use crate::{app::control::ViewDelta, direction::Direction, viewport::Viewport};

pub struct HelpState {
    viewport: Viewport,
}

impl HelpState {
    pub fn new() -> Self {
        Self {
            viewport: Viewport::new(),
        }
    }

    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    pub fn set_height(&mut self, height: usize) {
        self.viewport.fit(height, 0);
    }

    pub fn pan_vertically(&mut self, dir: Direction, delta: ViewDelta) {
        let delta = match delta {
            ViewDelta::Number { value } => usize::from(value),
            ViewDelta::Page => self.viewport.height(),
            ViewDelta::HalfPage => self.viewport.height().div_ceil(2),
            ViewDelta::Boundary if let Direction::Back = dir => usize::MAX,
            ViewDelta::Boundary => 0,
            ViewDelta::Match => unimplemented!("there is no result jumping for help"),
        };
        for _ in 0..delta {
            self.viewport.pan_vertical(dir);
        }
    }
}
