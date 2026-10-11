use crate::{
    Config, Error,
    framebuffer::Framebuffer,
    input::Evdev,
    wake::{Proxy, WakeQueue},
};
use slint::{
    PhysicalSize, Window, WindowSize,
    platform::{
        self, Platform, PlatformError, Renderer, WindowAdapter, WindowEvent, WindowProperties,
        software_renderer::{RepaintBufferType, SoftwareRenderer},
    },
};
use std::{
    cell::{Cell, RefCell},
    io,
    os::fd::AsRawFd,
    rc::{Rc, Weak},
    sync::Arc,
    time::{Duration, Instant},
};

pub(crate) struct BridgeWindow {
    window: Window,
    renderer: SoftwareRenderer,
    size: PhysicalSize,
    visible: Cell<bool>,
    dirty: Cell<bool>,
    incompatible_layout: Cell<bool>,
    wake: Arc<WakeQueue>,
}

impl BridgeWindow {
    fn new(size: PhysicalSize, config: &Config, wake: Arc<WakeQueue>) -> Rc<Self> {
        Rc::new_cyclic(|weak: &Weak<Self>| {
            let renderer =
                SoftwareRenderer::new_with_repaint_buffer_type(RepaintBufferType::ReusedBuffer);
            renderer.set_rendering_rotation(config.rotation.renderer());
            Self {
                window: Window::new(weak.clone()),
                renderer,
                size,
                visible: Cell::new(false),
                dirty: Cell::new(true),
                incompatible_layout: Cell::new(false),
                wake,
            }
        })
    }

    fn resize(&self) {
        self.window.dispatch_event(WindowEvent::Resized {
            size: self.size.to_logical(1.0),
        });
    }
}

impl WindowAdapter for BridgeWindow {
    fn window(&self) -> &Window {
        &self.window
    }
    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }
    fn size(&self) -> PhysicalSize {
        self.size
    }
    fn set_size(&self, _: WindowSize) {
        self.resize();
    }

    fn set_visible(&self, visible: bool) -> Result<(), PlatformError> {
        if visible && self.incompatible_layout.get() {
            return Err(PlatformError::from("window layout constraints do not fit the fixed panel size; use a responsive root Window".to_string()));
        }
        self.visible.set(visible);
        if visible {
            self.resize();
            self.window
                .dispatch_event(WindowEvent::WindowActiveChanged(true));
            self.request_redraw();
        } else {
            self.window
                .dispatch_event(WindowEvent::WindowActiveChanged(false));
        }
        self.wake
            .notify()
            .map_err(|e| PlatformError::from(e.to_string()))
    }

    fn request_redraw(&self) {
        self.dirty.set(true);
        // The queue retains a live eventfd while the window exists. EAGAIN only
        // means a wakeup is already pending; other failures surface in poll/read.
        let _ = self.wake.notify();
    }

    fn update_window_properties(&self, properties: WindowProperties<'_>) {
        let limits = properties.layout_constraints();
        let size = self.size.to_logical(1.0);
        self.incompatible_layout.set(
            limits
                .min
                .is_some_and(|min| min.width > size.width || min.height > size.height)
                || limits
                    .max
                    .is_some_and(|max| max.width < size.width || max.height < size.height),
        );
    }
}

pub(crate) struct BridgePlatform {
    pub(crate) window: Rc<BridgeWindow>,
    framebuffer: RefCell<Framebuffer>,
    inputs: RefCell<Vec<Evdev>>,
    initial_events: RefCell<Vec<WindowEvent>>,
    pub(crate) wake: Arc<WakeQueue>,
    frame_interval: Duration,
    timer_quantum: Duration,
    created: Cell<bool>,
    running: Cell<bool>,
}

impl BridgePlatform {
    pub(crate) fn new(
        config: Config,
        framebuffer: Framebuffer,
        timer_quantum: Duration,
    ) -> Result<Self, Error> {
        let size = framebuffer.layout.size;
        let mut inputs = Vec::new();
        let mut initial_events = Vec::new();
        for device in &config.inputs {
            let mut input = Evdev::open(device, size, config.rotation)?;
            input.resync(&mut initial_events)?;
            inputs.push(input);
        }
        let mut platform = Self::with_inputs(config, framebuffer, inputs, initial_events)?;
        platform.timer_quantum = timer_quantum;
        Ok(platform)
    }

    pub(crate) fn with_inputs(
        config: Config,
        framebuffer: Framebuffer,
        inputs: Vec<Evdev>,
        initial_events: Vec<WindowEvent>,
    ) -> Result<Self, Error> {
        let wake = framebuffer.wake();
        let window = BridgeWindow::new(
            config.rotation.logical_size(framebuffer.layout.size),
            &config,
            wake.clone(),
        );
        Ok(Self {
            window,
            framebuffer: RefCell::new(framebuffer),
            inputs: RefCell::new(inputs),
            initial_events: RefCell::new(initial_events),
            wake,
            frame_interval: config.frame_interval,
            timer_quantum: Duration::ZERO,
            created: Cell::new(false),
            running: Cell::new(false),
        })
    }

    fn run(&self) -> Result<(), Error> {
        let result = self.run_frames();
        // A quit or input/render error must still drain the submitted page.
        let completion = self.framebuffer.borrow_mut().wait_for_present();
        match (result, completion) {
            (Err(error), Err(Error::PresentationFailed(reason)))
                if !matches!(&error, Error::PresentationFailed(_)) =>
            {
                Err(Error::PresentationFailed(format!(
                    "{reason}; event loop also failed: {error}"
                )))
            }
            (_, Err(error)) | (Err(error), Ok(())) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    fn run_frames(&self) -> Result<(), Error> {
        let mut next_frame = Instant::now();
        let mut events = std::mem::take(&mut *self.initial_events.borrow_mut());
        loop {
            self.wake.drain_signal()?;
            self.framebuffer.borrow_mut().poll_present()?;
            if self.wake.quit_requested() {
                return Ok(());
            }
            // Slint requires timer/animation advancement before input and drawing.
            platform::update_timers_and_animations();
            for task in self.wake.tasks() {
                if self.wake.quit_requested() {
                    return Ok(());
                }
                task();
            }
            if self.wake.quit_requested() {
                return Ok(());
            }
            for input in self.inputs.borrow_mut().iter_mut() {
                input.read_ready(&mut events)?;
            }
            for event in events.drain(..) {
                self.window.window.dispatch_event(event);
            }
            // Input and queued callbacks can schedule zero-delay window-property
            // updates. Apply their layout constraints before allocating/drawing.
            platform::update_timers_and_animations();
            if self.wake.quit_requested() {
                return Ok(());
            }
            if self.window.incompatible_layout.get() {
                return Err(Error::InvalidConfig(
                    "window layout constraints do not fit the fixed panel size",
                ));
            }
            let now = Instant::now();
            if self.window.visible.get()
                && (self.window.dirty.get() || self.window.window.has_active_animations())
                && self.framebuffer.borrow().can_render()
                && now >= next_frame
            {
                self.window.dirty.set(false);
                next_frame = now + self.frame_interval;
                self.framebuffer
                    .borrow_mut()
                    .render(&self.window.renderer)?;
            }
            if self.window.visible.get() && self.framebuffer.borrow().ready_to_present() {
                self.framebuffer.borrow_mut().present()?;
            }
            let animating = self.window.visible.get() && self.window.window.has_active_animations();
            let redraw_pending = self.window.visible.get() && self.window.dirty.get();
            let mut timeout = platform::duration_until_next_timer_update();
            if (animating || redraw_pending) && self.framebuffer.borrow().can_render() {
                // One retained frame can be prepared while the worker owns
                // both display pages. Further rendering waits for submission.
                let frame_wait = next_frame.saturating_duration_since(Instant::now());
                timeout = Some(timeout.map_or(frame_wait, |timer| timer.min(frame_wait)));
            }
            if self.wake.has_tasks() {
                timeout = Some(Duration::ZERO);
            }
            self.wait(timeout)?;
        }
    }

    fn wait(&self, timeout: Option<Duration>) -> Result<(), Error> {
        let mut fds = vec![libc::pollfd {
            fd: self.wake.fd(),
            events: libc::POLLIN,
            revents: 0,
        }];
        fds.extend(self.inputs.borrow().iter().map(|input| libc::pollfd {
            fd: input.file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        }));
        if let Err(error) = crate::event_wait::wait(&mut fds, timeout, self.timer_quantum)
            && error.kind() != io::ErrorKind::Interrupted
        {
            return Err(Error::io("poll event loop", error));
        }
        if fds
            .iter()
            .any(|fd| fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0)
        {
            return Err(Error::io(
                "event-loop device disconnected",
                io::ErrorKind::BrokenPipe.into(),
            ));
        }
        Ok(())
    }
}

impl Platform for BridgePlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        if self.created.replace(true) {
            return Err(PlatformError::from(
                "only one fullscreen root window is supported".to_string(),
            ));
        }
        self.window
            .window
            .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });
        self.window.resize();
        Ok(self.window.clone())
    }

    fn run_event_loop(&self) -> Result<(), PlatformError> {
        if self.running.replace(true) {
            return Err(PlatformError::from(
                "event loop is already running".to_string(),
            ));
        }
        struct Guard<'a>(&'a BridgePlatform);
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                self.0.wake.finish();
                self.0.running.set(false);
            }
        }
        let _guard = Guard(self);
        self.wake.begin();
        self.run().map_err(|e| PlatformError::from(e.to_string()))
    }

    fn new_event_loop_proxy(&self) -> Option<Box<dyn platform::EventLoopProxy>> {
        Some(Box::new(Proxy(self.wake.clone())))
    }
}
