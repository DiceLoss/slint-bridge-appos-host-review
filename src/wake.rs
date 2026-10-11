use crate::Error;
use slint::{EventLoopError, platform::EventLoopProxy};
use std::{
    collections::VecDeque,
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    sync::{Arc, Mutex},
};

type Task = Box<dyn FnOnce() + Send>;

struct State {
    tasks: VecDeque<Task>,
    quit: bool,
    accepting: bool,
}

pub(crate) struct WakeQueue {
    fd: OwnedFd,
    state: Mutex<State>,
}

impl WakeQueue {
    pub(crate) fn new() -> Result<Arc<Self>, Error> {
        // SAFETY: eventfd returns a new descriptor, transferred exactly once.
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if fd < 0 {
            return Err(Error::io(
                "create event-loop wakeup",
                io::Error::last_os_error(),
            ));
        }
        Ok(Arc::new(Self {
            fd: unsafe { OwnedFd::from_raw_fd(fd) },
            state: Mutex::new(State {
                tasks: VecDeque::new(),
                quit: false,
                accepting: true,
            }),
        }))
    }

    pub(crate) fn fd(&self) -> i32 {
        self.fd.as_raw_fd()
    }

    pub(crate) fn notify(&self) -> io::Result<()> {
        let value = 1u64;
        loop {
            // SAFETY: eventfd consumes exactly one initialized u64.
            if unsafe { libc::write(self.fd(), (&value as *const u64).cast(), 8) } == 8 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::WouldBlock => return Ok(()), // Already readable.
                _ => return Err(error),
            }
        }
    }

    pub(crate) fn drain_signal(&self) -> Result<(), Error> {
        let mut value = 0u64;
        loop {
            // SAFETY: the destination is a valid writable u64.
            let n = unsafe { libc::read(self.fd(), (&mut value as *mut u64).cast(), 8) };
            if n == 8 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::WouldBlock => return Ok(()),
                _ => return Err(Error::io("read event-loop wakeup", error)),
            }
        }
    }

    pub(crate) fn tasks(&self) -> Vec<Task> {
        let mut state = self.state.lock().unwrap();
        let count = state.tasks.len().min(128);
        state.tasks.drain(..count).collect()
    }

    pub(crate) fn quit_requested(&self) -> bool {
        self.state.lock().unwrap().quit
    }
    pub(crate) fn has_tasks(&self) -> bool {
        !self.state.lock().unwrap().tasks.is_empty()
    }

    pub(crate) fn begin(&self) {
        let mut state = self.state.lock().unwrap();
        if !state.accepting {
            state.accepting = true;
            state.quit = false;
        }
    }

    pub(crate) fn finish(&self) {
        let discarded = {
            let mut state = self.state.lock().unwrap();
            state.accepting = false;
            std::mem::take(&mut state.tasks)
        };
        // Captured values may themselves interact with the proxy on drop.
        drop(discarded);
    }
}

pub(crate) struct Proxy(pub Arc<WakeQueue>);

impl EventLoopProxy for Proxy {
    fn quit_event_loop(&self) -> Result<(), EventLoopError> {
        {
            let mut state = self.0.state.lock().unwrap();
            if !state.accepting {
                return Err(EventLoopError::EventLoopTerminated);
            }
            state.quit = true;
        }
        self.0
            .notify()
            .map_err(|_| EventLoopError::EventLoopTerminated)
    }

    fn invoke_from_event_loop(&self, event: Task) -> Result<(), EventLoopError> {
        {
            let mut state = self.0.state.lock().unwrap();
            if !state.accepting {
                return Err(EventLoopError::EventLoopTerminated);
            }
            state.tasks.push_back(event);
        }
        self.0
            .notify()
            .map_err(|_| EventLoopError::EventLoopTerminated)
    }
}
