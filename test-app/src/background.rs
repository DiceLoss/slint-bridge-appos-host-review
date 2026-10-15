//! Application-side geometry for the public BackgroundWithHoles component.
use crate::{BackgroundGeometry, BackgroundHole, BackgroundRect};
use slint::{Model, ModelExt, ModelRc, VecModel};

/// Register the geometry callback before showing a window using BackgroundWithHoles.
pub fn install(geometry: BackgroundGeometry<'_>) {
    geometry.on_visible_regions(visible_regions);
}

fn visible_regions(
    width: i32,
    height: i32,
    holes: ModelRc<BackgroundHole>,
) -> ModelRc<BackgroundRect> {
    let mut regions = if width > 0 && height > 0 {
        vec![BackgroundRect {
            x: 0,
            y: 0,
            width,
            height,
        }]
    } else {
        Vec::new()
    };
    holes.model_tracker().track_row_count_changes();
    for hole in (0..holes.row_count())
        .filter_map(|row| holes.row_data_tracked(row))
        .filter(|h| h.visible && h.width > 0 && h.height > 0)
    {
        let left = hole.x.max(0);
        let top = hole.y.max(0);
        let right = (i64::from(hole.x) + i64::from(hole.width)).min(i64::from(width));
        let bottom = (i64::from(hole.y) + i64::from(hole.height)).min(i64::from(height));
        if i64::from(left) >= right || i64::from(top) >= bottom {
            continue;
        }
        let (right, bottom) = (right as i32, bottom as i32);
        let mut remaining = Vec::new();
        for r in regions {
            let x0 = r.x.max(left);
            let y0 = r.y.max(top);
            let x1 = (r.x + r.width).min(right);
            let y1 = (r.y + r.height).min(bottom);
            if x0 >= x1 || y0 >= y1 {
                remaining.push(r);
                continue;
            }
            for (x, y, width, height) in [
                (r.x, r.y, r.width, y0 - r.y),
                (r.x, y1, r.width, r.y + r.height - y1),
                (r.x, y0, x0 - r.x, y1 - y0),
                (x1, y0, r.x + r.width - x1, y1 - y0),
            ] {
                if width > 0 && height > 0 {
                    remaining.push(BackgroundRect {
                        x,
                        y,
                        width,
                        height,
                    });
                }
            }
        }
        regions = remaining;
    }
    ModelRc::new(VecModel::from(regions))
}
