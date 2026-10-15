//! Interactive two-region example using the application-side background component.
use crate::{BackgroundGeometry, BackgroundHole, BackgroundHolesDemo, background};
use slint::{ComponentHandle, Model, VecModel};
use std::rc::Rc;

fn initial_holes() -> Vec<BackgroundHole> {
    vec![
        BackgroundHole {
            x: 97,
            y: 143,
            width: 350,
            height: 241,
            visible: true,
        },
        BackgroundHole {
            x: 571,
            y: 189,
            width: 337,
            height: 249,
            visible: true,
        },
    ]
}

/// Install background geometry and the demo's move, resize, visibility and dialog actions.
pub fn configure(app: &BackgroundHolesDemo) {
    background::install(app.global::<BackgroundGeometry>());
    let holes = Rc::new(VecModel::from(initial_holes()));
    app.set_holes(holes.clone().into());
    let weak = app.as_weak();
    app.on_action(move |action| {
        let Some(app) = weak.upgrade() else {
            return;
        };
        match action {
            0..=5 => {
                let row = (action / 3) as usize;
                let mut hole = holes.row_data(row).unwrap();
                let initial = &initial_holes()[row];
                match action % 3 {
                    0 => {
                        hole.x = if hole.x == initial.x {
                            initial.x + if row == 0 { 76 } else { -78 }
                        } else {
                            initial.x
                        };
                        hole.y = if hole.y == initial.y {
                            initial.y - 32
                        } else {
                            initial.y
                        };
                    }
                    1 => {
                        hole.width = if hole.width == initial.width {
                            initial.width + if row == 0 { -87 } else { 84 }
                        } else {
                            initial.width
                        };
                        hole.height = if hole.height == initial.height {
                            initial.height + if row == 0 { -42 } else { 38 }
                        } else {
                            initial.height
                        };
                    }
                    _ => hole.visible = !hole.visible,
                }
                holes.set_row_data(row, hole);
            }
            6 => app.set_modal(true),
            7 => {
                holes.set_vec(initial_holes());
                app.set_modal(false);
                app.set_presses(0);
            }
            8 => app.set_presses(app.get_presses() + 1),
            _ => return,
        }
        app.set_revision(app.get_revision() + 1);
    });
}
