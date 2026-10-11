use slint::{LogicalPosition, PhysicalSize, platform::software_renderer::RenderingRotation};

/// Clockwise rotation from logical UI coordinates into physical panel coordinates.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    #[default]
    None,
    Clockwise90,
    Clockwise180,
    Clockwise270,
}

impl Rotation {
    pub(crate) fn logical_size(self, panel: PhysicalSize) -> PhysicalSize {
        match self {
            Self::Clockwise90 | Self::Clockwise270 => PhysicalSize::new(panel.height, panel.width),
            _ => panel,
        }
    }

    pub(crate) fn renderer(self) -> RenderingRotation {
        match self {
            Self::None => RenderingRotation::NoRotation,
            Self::Clockwise90 => RenderingRotation::Rotate90,
            Self::Clockwise180 => RenderingRotation::Rotate180,
            Self::Clockwise270 => RenderingRotation::Rotate270,
        }
    }

    pub(crate) fn input(self, panel: PhysicalSize, x: f32, y: f32) -> LogicalPosition {
        let w = panel.width.saturating_sub(1) as f32;
        let h = panel.height.saturating_sub(1) as f32;
        let (x, y) = (x.clamp(0.0, w), y.clamp(0.0, h));
        let (x, y) = match self {
            Self::None => (x, y),
            Self::Clockwise90 => (y, w - x),
            Self::Clockwise180 => (w - x, h - y),
            Self::Clockwise270 => (h - y, x),
        };
        LogicalPosition::new(x, y)
    }
}
