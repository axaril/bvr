use crate::{
    app::{control::ViewDelta, filters::FilterExportSet},
    cursor::{Cursor, CursorAnchor, CursorState},
    direction::Direction,
    view_bounds::ViewBounds,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

const FILTER_FILE: &str = "filters.toml";

#[derive(Serialize, Deserialize, Default)]
struct LoadedFilterData {
    persistent: bool,
    persistent_filter: Option<FilterExportSet>,
    filters: Vec<FilterExportSet>,
}

pub struct FilterConfigState {
    inner: super::ConfigBase<LoadedFilterData>,
    bounds: ViewBounds,
    cursor: CursorState,
}

pub use FilterConfigState as State;

impl FilterConfigState {
    pub fn new() -> Self {
        Self {
            inner: super::ConfigBase::new(std::path::Path::new(FILTER_FILE)),
            bounds: ViewBounds::new(),
            cursor: CursorState::new(),
        }
    }

    pub fn set_persistent(&mut self, persistent: bool) -> Result<()> {
        self.inner.load_and_save(|data| {
            data.persistent = persistent;
        })
    }

    pub fn is_persistent(&self) -> bool {
        self.inner.read(|data| data.persistent).unwrap_or(false)
    }

    pub fn get_persistent_filter(&mut self) -> Result<Option<&FilterExportSet>> {
        self.inner.read(|data| data.persistent_filter.as_ref())
    }

    pub fn set_persistent_filter(&mut self, filter: FilterExportSet) -> Result<()> {
        self.inner.load_and_save(|data| {
            data.persistent_filter.replace(filter);
        })
    }

    pub fn filters(&self) -> &[FilterExportSet] {
        self.inner.read(|data| data.filters.as_ref()).unwrap_or(&[])
    }

    pub fn add_filter(&mut self, filter: FilterExportSet) -> Result<()> {
        self.inner.load_and_save(|data| {
            data.filters.push(filter);
        })
    }

    pub fn update_view_bounds(&mut self, height: usize) {
        self.bounds.fit(height, 0);
        self.bounds.clamp(self.filters().len());
    }

    pub fn view(&self) -> impl Iterator<Item = (usize, &FilterExportSet)> {
        self.filters()
            .iter()
            .enumerate()
            .skip(self.bounds.top())
            .take(self.bounds.height())
    }

    pub fn move_select(&mut self, dir: Direction, select: bool, delta: ViewDelta) {
        let delta = match delta {
            ViewDelta::Number { value } => usize::from(value),
            ViewDelta::Page => self.bounds.height(),
            ViewDelta::HalfPage => self.bounds.height().div_ceil(2),
            ViewDelta::Boundary => usize::MAX,
            ViewDelta::Match => unimplemented!("there is no result jumping for filters"),
        };
        match dir {
            Direction::Back => self.cursor.back(select, |i| i.saturating_sub(delta)),
            Direction::Next => self.cursor.forward(select, |i| i.saturating_add(delta)),
        }
        self.cursor.clamp(self.filters().len().saturating_sub(1));
        let i = match self.cursor.state() {
            Cursor::Singleton(i)
            | Cursor::Selection(i, _, CursorAnchor::End)
            | Cursor::Selection(_, i, CursorAnchor::Start) => i,
        };
        self.bounds.jump_vertically_to(i);
    }

    #[allow(dead_code)]
    pub fn clear_filters(&mut self) -> Result<()> {
        self.cursor = CursorState::new();
        self.inner.load_and_save(|data| {
            data.filters.clear();
        })
    }

    pub fn selected_filter(&self) -> Option<&FilterExportSet> {
        match self.cursor.state() {
            Cursor::Singleton(i) => self.filters().get(i),
            _ => None,
        }
    }

    pub fn selected_filter_indices(&self) -> std::ops::Range<usize> {
        match self.cursor.state() {
            Cursor::Singleton(i) => i..i + 1,
            Cursor::Selection(start, end, _) => start..end + 1,
        }
    }

    pub fn remove_filters(&mut self, range: std::ops::Range<usize>) -> Result<()> {
        let len = self.inner.load_read_save(|data| {
            data.filters.drain(range);
            data.filters.len()
        })?;
        self.cursor.clamp(len.unwrap_or(0).saturating_sub(1));
        Ok(())
    }

    #[allow(dead_code)]
    pub fn remove_filter(&mut self, index: usize) -> Result<()> {
        self.remove_filters(index..index + 1)
    }

    pub fn cursor(&self) -> &CursorState {
        &self.cursor
    }
}
