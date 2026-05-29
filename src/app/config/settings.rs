use anyhow::Result;
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
}

impl Default for LoadedSettings {
    fn default() -> Self {
        Self {
            show_gutter: Self::default_show_gutter(),
            mouse_capture: Self::default_mouse_capture(),
            persist_filter: Self::default_persist_filter(),
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
}
