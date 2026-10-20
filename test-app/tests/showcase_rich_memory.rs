use slint::platform::{
    Platform, PointerEventButton, WindowAdapter, WindowEvent,
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
};
use slint::{
    ComponentHandle, LogicalPosition, Model, ModelRc, ModelTracker, PhysicalSize, VecModel,
};
use slint_bridge_test_app::{DeviceRow, Showcase, showcase::configure};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

struct Host(Rc<MinimalSoftwareWindow>);
impl Platform for Host {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

struct ObservedRows {
    rows: VecModel<DeviceRow>,
    reads: RefCell<BTreeSet<usize>>,
}
impl Model for ObservedRows {
    type Data = DeviceRow;
    fn row_count(&self) -> usize {
        self.rows.row_count()
    }
    fn row_data(&self, row: usize) -> Option<DeviceRow> {
        self.reads.borrow_mut().insert(row);
        self.rows.row_data(row)
    }
    fn model_tracker(&self) -> &dyn ModelTracker {
        self.rows.model_tracker()
    }
}

fn click(app: &Showcase, x: f32, y: f32) {
    for event in [
        WindowEvent::PointerPressed {
            position: LogicalPosition::new(x, y),
            button: PointerEventButton::Left,
        },
        WindowEvent::PointerReleased {
            position: LogicalPosition::new(x, y),
            button: PointerEventButton::Left,
        },
    ] {
        app.window().dispatch_event(event);
    }
}

#[test]
fn rich_list_only_requests_viewport_rows_and_releases_inactive_assets() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(Host(window.clone()))).unwrap();
    let app = Showcase::new().unwrap();
    configure(&app).stop();
    app.invoke_navigate(7);
    let rows = Rc::new(ObservedRows {
        rows: VecModel::from(app.get_rich_rows().iter().collect::<Vec<_>>()),
        reads: RefCell::default(),
    });
    app.set_rich_rows(ModelRc::from(rows.clone()));
    app.show().unwrap();
    window.set_size(PhysicalSize::new(1024, 600));
    app.window().take_snapshot().unwrap();
    assert!(
        rows.reads.borrow().len() <= 8,
        "initial viewport requested {} of 100 rows",
        rows.reads.borrow().len()
    );
    // Moving the visible range must not rebind every surviving card's text.
    rows.reads.borrow_mut().clear();
    app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(500., 350.),
        delta_x: 0.,
        delta_y: -240.,
    });
    app.window().take_snapshot().unwrap();
    assert!(
        rows.reads.borrow().len() <= 2,
        "unchanged visible rows were rebound: {:?}",
        rows.reads.borrow()
    );
    app.window().dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(500., 350.),
        delta_x: 0.,
        delta_y: 100000.,
    });
    app.window().take_snapshot().unwrap();
    assert_eq!(app.get_rich_backgrounds().row_count(), 2);
    assert_eq!(
        app.get_rich_backgrounds().row_data(0).unwrap().size().width,
        800
    );

    for horizontal in [false, true] {
        if horizontal {
            click(&app, 740., 138.);
            app.window().take_snapshot().unwrap();
            assert_eq!(app.get_rich_backgrounds().row_count(), 2);
            assert_eq!(
                app.get_rich_backgrounds().row_data(0).unwrap().size().width,
                244
            );
        }
        for (delta, endpoint) in [(-100000., 99), (100000., 0)] {
            rows.reads.borrow_mut().clear();
            app.window().dispatch_event(WindowEvent::PointerScrolled {
                position: LogicalPosition::new(500., 350.),
                delta_x: if horizontal { delta } else { 0. },
                delta_y: if horizontal { 0. } else { delta },
            });
            app.window().take_snapshot().unwrap();
            let reads = rows.reads.borrow();
            assert!(reads.len() <= 8, "scroll requested {} rows", reads.len());
            assert!(
                reads.contains(&endpoint),
                "endpoint {endpoint} missing: {reads:?}"
            );
        }
    }
    let mut changed = rows.rows.row_data(99).unwrap();
    changed.name = "Preserved update".into();
    rows.rows.set_row_data(99, changed);
    app.invoke_select_rich(100);
    app.invoke_navigate(1);
    app.window().take_snapshot().unwrap();
    assert_eq!(app.get_rich_backgrounds().row_count(), 0);
    assert_eq!(app.get_rich_icons().row_count(), 0);
    app.invoke_navigate(7);
    app.window().take_snapshot().unwrap();
    assert_eq!(app.get_rich_backgrounds().row_count(), 2);
    assert_eq!(
        app.get_rich_backgrounds().row_data(0).unwrap().size().width,
        800
    );
    assert_eq!(app.get_rich_icons().row_count(), 5);
    assert_eq!(app.get_rich_selected_id(), 100);
    assert_eq!(
        app.get_rich_rows().row_data(99).unwrap().name,
        "Preserved update"
    );

    rows.rows.set_vec(Vec::new());
    app.window().take_snapshot().unwrap();
    rows.rows.set_vec(vec![DeviceRow {
        id: 1,
        ..Default::default()
    }]);
    app.window().take_snapshot().unwrap();
    click(&app, 400., 230.);
    assert_eq!(app.get_rich_selected_id(), 1);
}
