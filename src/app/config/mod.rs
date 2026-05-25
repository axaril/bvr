mod base;
pub mod filters;
mod widget;

use anyhow::Result;
use std::path::PathBuf;

use base::ConfigBase;
pub use widget::ConfigViewerWidget as Widget;

fn storage_dir(app_id: &str) -> Option<PathBuf> {
    directories_next::ProjectDirs::from("", "", app_id)
        .map(|proj_dirs| proj_dirs.data_dir().to_path_buf())
}

fn storage_dir_create(app_id: &str) -> Result<PathBuf> {
    let path = storage_dir(app_id).ok_or(bvr_core::err::Error::Internal)?;
    std::fs::create_dir_all(&path)?;
    Ok(path)
}
