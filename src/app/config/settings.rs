use anyhow::Result;
use crate::text::{DEFAULT_TAB_WIDTH, MAX_TAB_WIDTH};
use serde::{Deserialize, Serialize};

const FILE: &str = "config.toml";

#[derive(Serialize, Deserialize)]
struct LoadedSettings {
    #[serde(default = "LoadedSettings::default_show_gutter")]
    show_gutter: bool,
    #[serde(default = "LoadedSettings::default_mouse_capture")]
    mouse_capture: bool,
    #[serde(default = "LoadedSettings::default_persist_filter")]
    persist_filter: bool,
    #[serde(default = "LoadedSettings::default_wrap")]
    wrap: bool,
    #[serde(default = "LoadedSettings::default_tab_width")]
    tab_width: usize,
}

impl Default for LoadedSettings {
    fn default() -> Self {
        Self {
            show_gutter: Self::default_show_gutter(),
            mouse_capture: Self::default_mouse_capture(),
            persist_filter: Self::default_persist_filter(),
            wrap: Self::default_wrap(),
            tab_width: Self::default_tab_width(),
        }
    }
}

impl LoadedSettings {
    fn default_show_gutter() -> bool {
        true
    }

    fn default_mouse_capture() -> bool {
        true
    }

    fn default_persist_filter() -> bool {
        false
    }

    fn default_wrap() -> bool {
        false
    }

    fn default_tab_width() -> usize {
        DEFAULT_TAB_WIDTH
    }
}

pub struct SettingsConfigState {
    inner: super::ConfigBase<LoadedSettings>,
}

impl SettingsConfigState {
    pub fn new() -> Self {
        Self {
            inner: super::ConfigBase::new(std::path::Path::new(FILE)),
        }
    }

    pub fn show_gutter(&self) -> bool {
        self.inner.read(|data| data.show_gutter).unwrap_or(true)
    }

    #[allow(dead_code)]
    pub fn set_show_gutter(&mut self, show: bool) -> Result<()> {
        self.inner.load_and_save(|data| {
            data.show_gutter = show;
        })
    }

    pub fn toggle_show_gutter(&mut self) -> Result<bool> {
        self.inner.load_read_save(|data| {
            data.show_gutter = !data.show_gutter;
            data.show_gutter
        })
    }

    pub fn mouse_capture(&self) -> bool {
        self.inner.read(|data| data.mouse_capture).unwrap_or(true)
    }

    pub fn set_mouse_capture(&mut self, capture: bool) -> Result<()> {
        self.inner.load_and_save(|data| {
            data.mouse_capture = capture;
        })
    }

    pub fn persist_filter(&self) -> bool {
        self.inner.read(|data| data.persist_filter).unwrap_or(false)
    }

    #[allow(dead_code)]
    pub fn set_persist_filter(&mut self, persistent: bool) -> Result<()> {
        self.inner.load_and_save(|data| {
            data.persist_filter = persistent;
        })
    }

    pub fn toggle_persist_filter(&mut self) -> Result<bool> {
        self.inner.load_read_save(|data| {
            data.persist_filter = !data.persist_filter;
            data.persist_filter
        })
    }

    pub fn wrap(&self) -> bool {
        self.inner.read(|data| data.wrap).unwrap_or(false)
    }

    pub fn toggle_wrap(&mut self) -> Result<bool> {
        self.inner.load_read_save(|data| {
            data.wrap = !data.wrap;
            data.wrap
        })
    }

    /// Columns between tab stops, kept within `1..=MAX_TAB_WIDTH` however the config file reads.
    pub fn tab_width(&self) -> usize {
        self.inner
            .read(|data| data.tab_width)
            .unwrap_or(DEFAULT_TAB_WIDTH)
            .clamp(1, MAX_TAB_WIDTH)
    }

    pub fn set_tab_width(&mut self, tab_width: usize) -> Result<()> {
        self.inner.load_and_save(|data| {
            data.tab_width = tab_width;
        })
    }
}
