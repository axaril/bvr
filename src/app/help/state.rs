use crate::{app::control::ViewDelta, direction::Direction, view_bounds::ViewBounds};

pub struct HelpState {
    bounds: ViewBounds,
}

impl HelpState {
    pub fn new() -> Self {
        Self {
            bounds: ViewBounds::new(),
        }
    }

    pub fn view_bounds(&self) -> &ViewBounds {
        &self.bounds
    }

    pub fn set_height(&mut self, height: usize) {
        self.bounds.fit(height, 0);
    }

    pub fn pan_vertical(&mut self, dir: Direction, delta: ViewDelta) {
        let delta = match delta {
            ViewDelta::Number { value } => usize::from(value),
            ViewDelta::Page => self.bounds.height(),
            ViewDelta::HalfPage => self.bounds.height().div_ceil(2),
            ViewDelta::Boundary if let Direction::Back = dir => usize::MAX,
            ViewDelta::Boundary => 0,
            ViewDelta::Match => unimplemented!("there is no result jumping for help"),
        };
        self.bounds.pan_vertical(dir, delta);
    }
}
