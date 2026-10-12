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

#[test]
fn draining_wakeup_preserves_queued_work_and_later_notifications() {
    let queue = WakeQueue::new().unwrap();
    let executed = Arc::new(Mutex::new(Vec::new()));
    for value in 0..129 {
        let output = executed.clone();
        Proxy(queue.clone())
            .invoke_from_event_loop(Box::new(move || output.lock().unwrap().push(value)))
            .unwrap();
    }
    queue.drain_signal().unwrap();
    for task in queue.tasks() {
        task();
    }
    assert!(
        queue.has_tasks(),
        "the eventfd counter is not the task queue"
    );
    let output = executed.clone();
    Proxy(queue.clone())
        .invoke_from_event_loop(Box::new(move || output.lock().unwrap().push(129)))
        .unwrap();
    let mut fd = libc::pollfd {
        fd: queue.fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    assert_eq!(unsafe { libc::poll(&mut fd, 1, 0) }, 1);
    queue.drain_signal().unwrap();
    while queue.has_tasks() {
        for task in queue.tasks() {
            task();
        }
    }
    assert_eq!(*executed.lock().unwrap(), (0..130).collect::<Vec<_>>());
}

#[test]
fn task_callbacks_can_post_and_shutdown_without_holding_the_queue_lock() {
    let queue = WakeQueue::new().unwrap();
    let (released, unlocked) = mpsc::channel();
    let repost = queue.clone();
    Proxy(queue.clone())
        .invoke_from_event_loop(Box::new(move || {
            let worker = thread::spawn(move || {
                Proxy(repost.clone())
                    .invoke_from_event_loop(Box::new(|| {}))
                    .unwrap();
                Proxy(repost).quit_event_loop().unwrap();
                released.send(()).unwrap();
            });
            unlocked.recv_timeout(Duration::from_secs(1)).unwrap();
            worker.join().unwrap();
        }))
        .unwrap();
    for task in queue.tasks() {
        task();
    }
    assert!(queue.quit_requested());
    queue.finish();
}
