use crate::{
    app::{control::ViewDelta, filters::FilterExportSet},
    cursor::{Cursor, CursorState, SelectionOrigin},
    direction::Direction,
    view_bounds::ViewBounds,
};

use super::{APP_ID, FILTER_FILE, storage_dir_create};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{cell::OnceCell, path::PathBuf};

pub struct FilterConfigState {
    path: Option<PathBuf>,
    state: OnceCell<LoadedFilterData>,
    bounds: ViewBounds,
    cursor: CursorState,
}

#[derive(Serialize, Deserialize, Default)]
struct LoadedFilterData {
    persistent: bool,
    persistent_filter: Option<FilterExportSet>,
    filters: Vec<FilterExportSet>,
}

impl FilterConfigState {
    pub fn new() -> Self {
        Self {
            path: storage_dir_create(APP_ID)
                .map(|path| path.join(FILTER_FILE))
                .ok(),
            state: OnceCell::new(),
            bounds: ViewBounds::new(),
            cursor: CursorState::new(),
        }
    }

    fn load(&self) -> LoadedFilterData {
        self.path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|string| toml::from_str::<LoadedFilterData>(string.as_str()).ok())
            .unwrap_or_else(LoadedFilterData::default)
    }

    fn load_read_save<F, R>(&mut self, f: F) -> Result<Option<R>>
    where
        F: FnOnce(&mut LoadedFilterData) -> R,
    {
        // TODO: get rid of once OnceCell::get_mut_or_init stabilizes
        self.state.get_or_init(|| self.load());
        // Safety: get or init should not fail
        let data = unsafe { self.state.get_mut().unwrap_unchecked() };

        let result = f(data);

        let Some(path) = self.path.as_ref() else {
            return Ok(None);
        };
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        let mut writer = std::io::BufWriter::new(file);
        let toml_str = toml::to_string(data)?;
        std::io::Write::write_all(&mut writer, toml_str.as_bytes())?;
        Ok(Some(result))
    }

    fn load_and_save<F, R>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut LoadedFilterData) -> R,
    {
        self.load_read_save(f).map(|_| ())
    }

    fn read<'a, F, R>(&'a self, f: F) -> Result<R>
    where
        F: FnOnce(&'a LoadedFilterData) -> R,
    {
        Ok(f(self.state.get_or_init(|| self.load())))
    }

    pub fn set_persistent(&mut self, persistent: bool) -> Result<()> {
        self.load_and_save(|data| {
            data.persistent = persistent;
        })
    }

    pub fn is_persistent(&self) -> bool {
        self.read(|data| data.persistent).unwrap_or(false)
    }

    pub fn get_persistent_filter(&mut self) -> Result<Option<&FilterExportSet>> {
        self.read(|data| data.persistent_filter.as_ref())
    }

    pub fn set_persistent_filter(&mut self, filter: FilterExportSet) -> Result<()> {
        self.load_and_save(|data| {
            data.persistent_filter.replace(filter);
        })
    }

    pub fn filters(&self) -> &[FilterExportSet] {
        self.read(|data| data.filters.as_ref()).unwrap_or(&[])
    }

    pub fn add_filter(&mut self, filter: FilterExportSet) -> Result<()> {
        self.load_and_save(|data| {
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
            | Cursor::Selection(i, _, SelectionOrigin::Left)
            | Cursor::Selection(_, i, SelectionOrigin::Right) => i,
        };
        self.bounds.jump_vertically_to(i);
    }

    #[allow(dead_code)]
    pub fn clear_filters(&mut self) -> Result<()> {
        self.cursor = CursorState::new();
        self.load_and_save(|data| {
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
        let len = self.load_read_save(|data| {
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
