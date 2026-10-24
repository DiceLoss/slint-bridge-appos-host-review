use crate::{
    Config, Error, Rotation,
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

thread_local! {
    static ACTIVE_WINDOW: RefCell<Weak<BridgeWindow>> = const { RefCell::new(Weak::new()) };
    static FIRST_FRAME_ONLY: Cell<bool> = const { Cell::new(false) };
    static FIRST_FRAME_SUBMITTED: Cell<bool> = const { Cell::new(false) };
}

#[cfg(feature = "appos-host")]
pub(crate) fn request_redraw() {
    ACTIVE_WINDOW.with(|active| {
        if let Some(window) = active.borrow().upgrade() {
            window.request_redraw();
        }
    });
}

#[cfg(feature = "appos-host")]
pub(crate) fn request_full_redraw() {
    ACTIVE_WINDOW.with(|active| {
        if let Some(window) = active.borrow().upgrade() {
            window.force_full_repaint.set(true);
            window.request_redraw();
        }
    });
}

#[cfg(feature = "appos-host")]
pub(crate) fn release_graphics_caches() {
    ACTIVE_WINDOW.with(|active| {
        if let Some(window) = active.borrow().upgrade() {
            window.renderer.clear_font_caches();
            window.renderer.clear_image_caches();
            window.force_full_repaint.set(true);
            window.request_redraw();
        }
    });
}

#[cfg(feature = "appos-host")]
pub(crate) fn run_until_first_presented_frame() -> Result<(), slint::PlatformError> {
    let already_active = FIRST_FRAME_ONLY.with(|mode| mode.replace(true));
    if already_active {
        return Err(slint::PlatformError::Other(
            "first-frame event loop is already active".into(),
        ));
    }
    FIRST_FRAME_SUBMITTED.with(|submitted| submitted.set(false));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FIRST_FRAME_ONLY.with(|mode| mode.set(false));
            FIRST_FRAME_SUBMITTED.with(|submitted| submitted.set(false));
        }
    }
    let _reset = Reset;
    slint::run_event_loop()
}

pub(crate) struct BridgeWindow {
    window: Window,
    renderer: SoftwareRenderer,
    size: PhysicalSize,
    visible: Cell<bool>,
    dirty: Cell<bool>,
    force_full_repaint: Cell<bool>,
    incompatible_layout: Cell<bool>,
    rolling_back_show: Cell<bool>,
    wake: Arc<WakeQueue>,
}

impl BridgeWindow {
    fn new(
        size: PhysicalSize,
        rotation: Rotation,
        wake: Arc<WakeQueue>,
        font_paths: &[std::path::PathBuf],
    ) -> Result<Rc<Self>, PlatformError> {
        let renderer =
            SoftwareRenderer::new_with_repaint_buffer_type(RepaintBufferType::ReusedBuffer);
        renderer.set_rendering_rotation(rotation.renderer());
        for path in font_paths {
            (&renderer as &dyn Renderer)
                .register_font_from_path(path)
                .map_err(|error| PlatformError::from(error.to_string()))?;
        }
        Ok(Rc::new_cyclic(|weak: &Weak<Self>| Self {
            window: Window::new(weak.clone()),
            renderer,
            size,
            visible: Cell::new(false),
            dirty: Cell::new(true),
            force_full_repaint: Cell::new(false),
            incompatible_layout: Cell::new(false),
            rolling_back_show: Cell::new(false),
            wake,
        }))
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
            // Slint retains the visible component before calling set_visible.
            // Balance that reference even if show fails before the loop starts.
            struct Rollback<'a>(&'a Cell<bool>, bool);
            impl Drop for Rollback<'_> {
                fn drop(&mut self) {
                    self.0.set(self.1);
                }
            }
            let _rollback = Rollback(
                &self.rolling_back_show,
                self.rolling_back_show.replace(true),
            );
            if let Err(error) = self.window.hide() {
                eprintln!("slint-bridge: hide after rejected show: {error}");
            }
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
    // Window owns a Slint context, which owns this platform: keep this edge weak.
    window: RefCell<Weak<BridgeWindow>>,
    size: PhysicalSize,
    rotation: Rotation,
    framebuffer: RefCell<Framebuffer>,
    inputs: RefCell<Vec<Evdev>>,
    initial_events: RefCell<Vec<WindowEvent>>,
    pub(crate) wake: Arc<WakeQueue>,
    frame_interval: Duration,
    timer_quantum: Duration,
    created: Cell<bool>,
    running: Cell<bool>,
    pending_frame_started: Cell<Option<Instant>>,
    #[cfg(feature = "appos-host")]
    host_hooks: Option<Arc<dyn crate::host::HostHooks>>,
    #[cfg(feature = "appos-host")]
    font_paths: Vec<std::path::PathBuf>,
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
        let mut platform = Self::with_inputs(config, framebuffer, inputs, initial_events);
        platform.timer_quantum = timer_quantum;
        Ok(platform)
    }

    pub(crate) fn with_inputs(
        config: Config,
        framebuffer: Framebuffer,
        inputs: Vec<Evdev>,
        initial_events: Vec<WindowEvent>,
    ) -> Self {
        let wake = framebuffer.wake();
        let size = config.rotation.logical_size(framebuffer.layout.size);
        Self {
            window: RefCell::new(Weak::new()),
            size,
            rotation: config.rotation,
            framebuffer: RefCell::new(framebuffer),
            inputs: RefCell::new(inputs),
            initial_events: RefCell::new(initial_events),
            wake,
            frame_interval: config.frame_interval,
            timer_quantum: Duration::ZERO,
            created: Cell::new(false),
            running: Cell::new(false),
            pending_frame_started: Cell::new(None),
            #[cfg(feature = "appos-host")]
            host_hooks: None,
            #[cfg(feature = "appos-host")]
            font_paths: Vec::new(),
        }
    }

    #[cfg(feature = "appos-host")]
    pub(crate) fn with_host_inputs(
        config: Config,
        framebuffer: Framebuffer,
        inputs: Vec<Evdev>,
        initial_events: Vec<WindowEvent>,
        font_paths: Vec<std::path::PathBuf>,
        hooks: Option<Arc<dyn crate::host::HostHooks>>,
    ) -> Self {
        let mut platform = Self::with_inputs(config, framebuffer, inputs, initial_events);
        platform.host_hooks = hooks;
        platform.font_paths = font_paths;
        platform
    }

    fn run(&self) -> Result<(), Error> {
        let result = self.run_frames();
        // A quit or input/render error must still drain the submitted page.
        let completion = self.framebuffer.borrow_mut().wait_for_present();
        if self.framebuffer.borrow_mut().discard_prepared()
            && let Some(window) = self.window.borrow().upgrade()
        {
            // A later run must render the current component state, even if the
            // caller uses run_event_loop directly without hiding the window.
            window.request_redraw();
        }
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
            let presented = self.framebuffer.borrow_mut().poll_presented()?;
            #[cfg(feature = "appos-host")]
            if presented
                && let Some(started_at) = self.pending_frame_started.take()
                && let Some(hooks) = &self.host_hooks
            {
                hooks.frame_presented(started_at, Instant::now());
            }
            if presented
                && FIRST_FRAME_ONLY.with(Cell::get)
                && FIRST_FRAME_SUBMITTED.with(Cell::get)
            {
                return Ok(());
            }
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
            let Some(window) = self.window.borrow().upgrade() else {
                // Timers and queued work may create the first root later. Keep
                // draining input without accumulating events for a missing UI.
                events.clear();
                self.wait(platform::duration_until_next_timer_update())?;
                continue;
            };
            for event in events.drain(..) {
                #[cfg(feature = "appos-host")]
                if let Some(hooks) = &self.host_hooks {
                    let host_event = match &event {
                        WindowEvent::PointerPressed { .. } => {
                            Some(crate::host::HostInputEvent::TouchPressed)
                        }
                        WindowEvent::PointerReleased { .. } => {
                            Some(crate::host::HostInputEvent::TouchReleased)
                        }
                        WindowEvent::PointerMoved { .. } => {
                            Some(crate::host::HostInputEvent::PointerMotion)
                        }
                        WindowEvent::KeyPressed { .. } | WindowEvent::KeyReleased { .. } => {
                            Some(crate::host::HostInputEvent::Key)
                        }
                        _ => None,
                    };
                    if host_event.is_some_and(|event| {
                        hooks.input(event, Instant::now())
                            == crate::host::HostInputDisposition::Consume
                    }) {
                        window.force_full_repaint.set(true);
                        window.request_redraw();
                        continue;
                    }
                }
                window.window.dispatch_event(event);
            }
            // Input and queued callbacks can schedule zero-delay window-property
            // updates. Apply their layout constraints before allocating/drawing.
            platform::update_timers_and_animations();
            if self.wake.quit_requested() {
                return Ok(());
            }
            if window.incompatible_layout.get() {
                return Err(Error::InvalidConfig(
                    "window layout constraints do not fit the fixed panel size",
                ));
            }
            let now = Instant::now();
            #[cfg(feature = "appos-host")]
            if let Some(hooks) = &self.host_hooks {
                if hooks.evaluate_idle(now) {
                    window.force_full_repaint.set(true);
                    window.request_redraw();
                }
                if crate::host::set_frame_policy(hooks.frame_policy()) {
                    window.force_full_repaint.set(true);
                    window.request_redraw();
                }
            }
            if window.visible.get()
                && (window.dirty.get() || window.window.has_active_animations())
                && self.framebuffer.borrow().can_render()
                && now >= next_frame
            {
                window.dirty.set(false);
                next_frame = now + self.frame_interval;
                let full_repaint = window.force_full_repaint.replace(false);
                if full_repaint {
                    window
                        .renderer
                        .set_repaint_buffer_type(RepaintBufferType::NewBuffer);
                }
                let render = self.framebuffer.borrow_mut().render(&window.renderer);
                if full_repaint {
                    window
                        .renderer
                        .set_repaint_buffer_type(RepaintBufferType::ReusedBuffer);
                }
                if render? {
                    self.pending_frame_started.set(Some(now));
                }
                // Rendering can occupy the UI thread for a full frame. Return
                // to input before starting framebuffer conversion/writeback.
                // The prepared frame prevents another render until submission.
                continue;
            }
            if window.visible.get() && self.framebuffer.borrow().ready_to_present() {
                let submitted = self
                    .framebuffer
                    .borrow_mut()
                    .present(Some(Duration::from_millis(1)))?;
                if submitted && FIRST_FRAME_ONLY.with(Cell::get) {
                    FIRST_FRAME_SUBMITTED.with(|first| first.set(true));
                }
                if self.framebuffer.borrow().ready_to_present() {
                    // A partially copied page is not submitted. Process input
                    // before the next slice, without sleeping or re-rendering.
                    continue;
                }
            }
            let animating = window.visible.get() && window.window.has_active_animations();
            let redraw_pending = window.visible.get() && window.dirty.get();
            let mut timeout = platform::duration_until_next_timer_update();
            #[cfg(feature = "appos-host")]
            if let Some(hooks) = &self.host_hooks
                && let Some(idle_wait) = hooks.until_idle_deadline(Instant::now())
            {
                timeout = Some(timeout.map_or(idle_wait, |timer| timer.min(idle_wait)));
            }
            if (animating || redraw_pending) && self.framebuffer.borrow().can_render() {
                // One retained frame can be prepared while the worker owns
                // both display pages. Further rendering waits for submission.
                let frame_wait = next_frame.saturating_duration_since(Instant::now());
                timeout = Some(timeout.map_or(frame_wait, |timer| timer.min(frame_wait)));
            }
            self.wait(timeout)?;
        }
    }

    fn wait(&self, timeout: Option<Duration>) -> Result<(), Error> {
        let timeout = if self.wake.has_tasks() {
            Some(Duration::ZERO)
        } else {
            timeout
        };
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
        #[cfg(feature = "appos-host")]
        let font_paths = self.font_paths.as_slice();
        #[cfg(not(feature = "appos-host"))]
        let font_paths = &[];
        let window = BridgeWindow::new(self.size, self.rotation, self.wake.clone(), font_paths)?;
        ACTIVE_WINDOW.with(|active| *active.borrow_mut() = Rc::downgrade(&window));
        *self.window.borrow_mut() = Rc::downgrade(&window);
        window
            .window
            .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });
        window.resize();
        Ok(window)
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
        let result = self.run().map_err(|e| PlatformError::from(e.to_string()));
        // ComponentHandle::run skips hide on error. A visible Slint window owns
        // its component, so release that reference even when input/present fails.
        if result.is_err()
            && let Some(window) = self.window.borrow().upgrade()
            && let Err(error) = window.window.hide()
        {
            eprintln!("slint-bridge: hide after event-loop failure: {error}");
        }
        result
    }

    fn new_event_loop_proxy(&self) -> Option<Box<dyn platform::EventLoopProxy>> {
        // hide() balances Slint's keepalive and asks for a new proxy to quit
        // when the last window closes. A rejected show is not a close: suppress
        // only that rollback request. The installed public proxy remains live,
        // preserving explicit quit requests and queued startup work.
        if self
            .window
            .borrow()
            .upgrade()
            .is_some_and(|window| window.rolling_back_show.get())
        {
            return None;
        }
        Some(Box::new(Proxy(self.wake.clone())))
    }
}
