//! One blocking board submission at a time, independent of pixel rendering.
use crate::{Error, backends::PresentationContract, framebuffer::Layout, wake::WakeQueue};
use std::{
    fs::File,
    io,
    os::fd::AsFd,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread::{self, JoinHandle},
};

pub(crate) struct Presentation {
    requests: Option<SyncSender<u32>>,
    completions: Receiver<io::Result<()>>,
    thread: Option<JoinHandle<()>>,
    pending: bool,
    failure: Option<String>,
    pub(crate) wake: Arc<WakeQueue>,
    // Keep the board session alive even if the worker panics. Framebuffer drops
    // its renderer/TDE resources before releasing this final reference.
    _contract: Arc<Mutex<PresentationContract>>,
}

impl Presentation {
    pub(crate) fn new(
        file: File,
        layout: Layout,
        contract: PresentationContract,
    ) -> Result<Self, Error> {
        let wake = WakeQueue::new()?;
        let (requests, incoming) = mpsc::sync_channel(1);
        let (finished, completions) = mpsc::sync_channel(1);
        let contract = Arc::new(Mutex::new(contract));
        let worker_contract = contract.clone();
        let signal = wake.clone();
        let thread = thread::Builder::new()
            .name("fb-present".into())
            .stack_size(64 * 1024)
            .spawn(move || {
                // Also wake the event loop on a panic/channel disconnect.
                struct WakeOnExit {
                    finished: Option<SyncSender<io::Result<()>>>,
                    signal: Arc<WakeQueue>,
                }
                impl Drop for WakeOnExit {
                    fn drop(&mut self) {
                        // Disconnect before waking, so a panic cannot race a
                        // poll that observes an empty but still open channel.
                        self.finished.take();
                        let _ = self.signal.notify();
                    }
                }
                let on_exit = WakeOnExit {
                    finished: Some(finished),
                    signal: signal.clone(),
                };
                while let Ok(offset) = incoming.recv() {
                    let result = match worker_contract.lock() {
                        Ok(mut contract) => (contract.complete)(file.as_fd(), offset, &layout),
                        Err(_) => Err(io::Error::other("presentation callback panicked")),
                    };
                    let failed = result.is_err();
                    if on_exit.finished.as_ref().unwrap().send(result).is_err() {
                        break;
                    }
                    let _ = signal.notify();
                    if failed {
                        break;
                    }
                }
            })
            .map_err(|e| Error::io("start presentation worker", e))?;
        Ok(Self {
            requests: Some(requests),
            completions,
            thread: Some(thread),
            pending: false,
            failure: None,
            wake,
            _contract: contract,
        })
    }

    pub(crate) fn pending(&self) -> bool {
        self.pending
    }

    #[cfg(feature = "nx5-tde")]
    pub(crate) fn quarantine_session(&self) {
        // An uncertain accelerator job may still use the SDK. Keep the board
        // session and its exclusive lock; dropping it would shut down VO/SYS.
        std::mem::forget(self._contract.clone());
    }

    fn healthy(&self) -> Result<(), Error> {
        match &self.failure {
            Some(error) => Err(Error::PresentationFailed(error.clone())),
            None => Ok(()),
        }
    }

    fn failed(&mut self, error: impl ToString) -> Error {
        let message = error.to_string();
        self.failure = Some(message.clone());
        Error::PresentationFailed(message)
    }

    pub(crate) fn submit(&mut self, offset: u32) -> Result<(), Error> {
        self.healthy()?;
        assert!(
            !self.pending,
            "only one display submission may be in flight"
        );
        if self.requests.as_ref().unwrap().send(offset).is_err() {
            return Err(self.failed("presentation worker disconnected"));
        }
        self.pending = true;
        Ok(())
    }

    pub(crate) fn complete(&mut self, wait: bool) -> Result<bool, Error> {
        self.healthy()?;
        if !self.pending {
            return Ok(false);
        }
        let result = if wait {
            self.completions
                .recv()
                .unwrap_or_else(|_| Err(io::Error::other("presentation worker disconnected")))
        } else {
            match self.completions.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => return Ok(false),
                Err(TryRecvError::Disconnected) => {
                    Err(io::Error::other("presentation worker disconnected"))
                }
            }
        };
        self.pending = false;
        result.map_err(|error| self.failed(error))?;
        Ok(true)
    }

    pub(crate) fn shutdown(&mut self) {
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Presentation {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::Duration,
    };

    struct Session(Arc<AtomicBool>);
    impl Drop for Session {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn shutdown_joins_the_ioctl_and_keeps_the_board_session_until_owner_drop() {
        let dropped = Arc::new(AtomicBool::new(false));
        let session = Session(dropped.clone());
        let (entered, started) = mpsc::sync_channel(1);
        let (release, gate) = mpsc::sync_channel(1);
        let contract = PresentationContract {
            validate_layout: Box::new(|_, f, v| crate::framebuffer::Layout::new(f, v)),
            complete: Box::new(move |_, _, _| {
                let _alive = &session;
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(3))
                    .map_err(io::Error::other)
            }),
        };
        let mut presenter = Presentation::new(
            File::open("/dev/null").unwrap(),
            {
                let (fixed, variable) = crate::tests::info(8, 6, 32);
                Layout::new(&fixed, &variable).unwrap()
            },
            contract,
        )
        .unwrap();
        presenter.submit(0).unwrap();
        started.recv_timeout(Duration::from_secs(3)).unwrap();
        let (joined, joined_rx) = mpsc::sync_channel(1);
        let observer = dropped.clone();
        let owner = thread::spawn(move || {
            presenter.shutdown();
            assert!(
                !observer.load(Ordering::SeqCst),
                "board owner must outlive worker shutdown"
            );
            presenter.complete(true).unwrap();
            drop(presenter);
            assert!(observer.load(Ordering::SeqCst));
            joined.send(()).unwrap();
        });
        assert!(joined_rx.try_recv().is_err());
        assert!(!dropped.load(Ordering::SeqCst));
        release.send(()).unwrap();
        joined_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        owner.join().unwrap();
    }

    #[test]
    fn worker_panic_wakes_poll_and_does_not_release_the_board_session() {
        let dropped = Arc::new(AtomicBool::new(false));
        let session = Session(dropped.clone());
        let contract = PresentationContract {
            validate_layout: Box::new(|_, f, v| crate::framebuffer::Layout::new(f, v)),
            complete: Box::new(move |_, _, _| {
                let _alive = &session;
                panic!("injected presentation callback panic");
            }),
        };
        let mut presenter = Presentation::new(
            File::open("/dev/null").unwrap(),
            {
                let (fixed, variable) = crate::tests::info(8, 6, 32);
                Layout::new(&fixed, &variable).unwrap()
            },
            contract,
        )
        .unwrap();
        presenter.submit(0).unwrap();
        let mut fd = libc::pollfd {
            fd: presenter.wake.fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(unsafe { libc::poll(&mut fd, 1, 3000) }, 1);
        assert!(matches!(
            presenter.complete(false),
            Err(Error::PresentationFailed(_))
        ));
        assert!(matches!(
            presenter.submit(600),
            Err(Error::PresentationFailed(_))
        ));
        assert!(!dropped.load(Ordering::SeqCst));
        presenter.shutdown();
        assert!(!dropped.load(Ordering::SeqCst));
        drop(presenter);
        assert!(dropped.load(Ordering::SeqCst));
    }
}
