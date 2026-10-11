use super::*;

#[test]
fn proxy_wakes_poll_and_rejects_after_shutdown() {
    let queue = WakeQueue::new().unwrap();
    let proxy = Proxy(queue.clone());
    let marker = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = marker.clone();
    thread::spawn(move || {
        proxy
            .invoke_from_event_loop(Box::new(move || {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
            }))
            .unwrap()
    })
    .join()
    .unwrap();
    let mut fd = libc::pollfd {
        fd: queue.fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    assert_eq!(unsafe { libc::poll(&mut fd, 1, 1000) }, 1);
    queue.drain_signal().unwrap();
    for task in queue.tasks() {
        task();
    }
    assert!(marker.load(std::sync::atomic::Ordering::SeqCst));
    queue.finish();
    assert!(
        Proxy(queue)
            .invoke_from_event_loop(Box::new(|| {}))
            .is_err()
    );
}
