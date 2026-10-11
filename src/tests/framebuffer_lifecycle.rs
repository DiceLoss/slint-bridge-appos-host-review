use super::*;
use std::{
    os::{fd::FromRawFd, unix::fs::MetadataExt},
    sync::{Mutex, mpsc},
    thread,
    time::Duration,
};

#[derive(Debug)]
struct DropEvent {
    name: &'static str,
    mapped: bool,
    file_open: bool,
}

type Events = Arc<Mutex<Vec<DropEvent>>>;

fn mapping_exists(inode: u64) -> bool {
    std::fs::read_to_string("/proc/self/maps")
        .unwrap()
        .lines()
        .any(|line| line.split_whitespace().nth(4).and_then(|n| n.parse().ok()) == Some(inode))
}

fn record(events: &Events, name: &'static str, inode: u64, fd: i32) {
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    // fstat accepts an already closed fd; compare the inode in case it was reused.
    let file_open =
        unsafe { libc::fstat(fd, &mut stat) } == 0 && u128::from(stat.st_ino) == u128::from(inode);
    events.lock().unwrap().push(DropEvent {
        name,
        mapped: mapping_exists(inode),
        file_open,
    });
}

struct Session {
    events: Events,
    inode: u64,
    fd: i32,
    dropped: mpsc::SyncSender<()>,
}

impl Drop for Session {
    fn drop(&mut self) {
        record(&self.events, "sdk", self.inode, self.fd);
        let _ = self.dropped.send(());
    }
}

#[test]
fn framebuffer_drop_drains_submission_before_releasing_resources() {
    for outcome in ["ok", "error", "panic"] {
        let events: Events = Arc::default();
        let observed = events.clone();
        let (entered, started) = mpsc::sync_channel(1);
        let (release, gate) = mpsc::sync_channel(1);
        let (dropping, drop_started) = mpsc::sync_channel(1);
        let (returned, done) = mpsc::sync_channel(1);
        let (session_dropped, session_done) = mpsc::sync_channel(1);
        // Construct and destroy on the owning thread: TDE resources are not Send.
        let owner = thread::spawn(move || {
            let fd = unsafe {
                libc::syscall(
                    libc::SYS_memfd_create,
                    c"framebuffer-drop-test".as_ptr(),
                    libc::MFD_CLOEXEC,
                )
            } as i32;
            assert!(fd >= 0);
            let file = unsafe { File::from_raw_fd(fd) };
            let (fixed, variable) = crate::tests::info(8, 6, 32);
            file.set_len(u64::from(fixed.smem_len)).unwrap();
            let inode = file.metadata().unwrap().ino();
            let session = Session {
                events: events.clone(),
                inode,
                fd,
                dropped: session_dropped,
            };
            let contract = PresentationContract {
                validate_layout: |_, _| Ok(()),
                complete: Box::new(move |_, _| {
                    entered.send(()).unwrap();
                    gate.recv_timeout(Duration::from_secs(3)).unwrap();
                    record(&session.events, "submission", session.inode, session.fd);
                    match outcome {
                        "error" => Err(io::Error::other("injected completion failure")),
                        "panic" => panic!("injected completion panic"),
                        _ => Ok(()),
                    }
                }),
            };
            let mut framebuffer =
                Framebuffer::from_file(file, Layout::new(&fixed, &variable).unwrap(), contract)
                    .unwrap();
            #[cfg(feature = "nx5-tde")]
            {
                let resources = events.clone();
                framebuffer.retained =
                    crate::backends::nx5::tde::tests::buffer_with_drop_observer(8, 6, move || {
                        record(&resources, "tde", inode, fd)
                    });
            }
            // Submit the initialized zero canvas through the real write/submit path.
            framebuffer.prepared_damage = Some(vec![Rect {
                x0: 0,
                y0: 0,
                x1: 8,
                y1: 6,
            }]);
            framebuffer.present().unwrap();
            started.recv_timeout(Duration::from_secs(3)).unwrap();
            dropping.send(inode).unwrap();
            drop(framebuffer);
            record(&events, "framebuffer", inode, fd);
            returned.send(()).unwrap();
        });
        let inode = drop_started.recv_timeout(Duration::from_secs(3)).unwrap();
        let early = done.recv_timeout(Duration::from_millis(100));
        let mapped_while_pending = mapping_exists(inode);
        release.send(()).unwrap();
        if early.is_err() {
            done.recv_timeout(Duration::from_secs(3)).unwrap();
        }
        session_done.recv_timeout(Duration::from_secs(3)).unwrap();
        owner.join().unwrap();
        let events = observed.lock().unwrap();
        assert!(
            early.is_err(),
            "drop returned with a pending submission: {events:?}"
        );
        assert!(
            mapped_while_pending,
            "mapping released during submission: {events:?}"
        );
        let names: Vec<_> = events.iter().map(|event| event.name).collect();
        #[cfg(feature = "nx5-tde")]
        assert_eq!(names, ["submission", "tde", "sdk", "framebuffer"]);
        #[cfg(not(feature = "nx5-tde"))]
        assert_eq!(names, ["submission", "sdk", "framebuffer"]);
        for event in events.iter().take(events.len() - 1) {
            assert!(event.mapped && event.file_open, "{outcome}: {events:?}");
        }
        let last = events.last().unwrap();
        assert!(
            !last.mapped && !last.file_open,
            "resources leaked: {events:?}"
        );
    }
}
