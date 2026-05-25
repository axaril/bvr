use super::storage_dir_create;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{cell::OnceCell, path::{Path, PathBuf}};

const APP_ID: &str = "bvr";

pub struct ConfigBase<T> {
    path: Option<PathBuf>,
    state: OnceCell<T>,
}

impl<T> ConfigBase<T> {
    pub fn new(path: &Path) -> Self {
        Self {
            path: storage_dir_create(APP_ID)
                .map(|base| base.join(path))
                .ok(),
            state: OnceCell::new(),
        }
    }
}

impl<T> ConfigBase<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Default,
{
    pub fn load(&self) -> T {
        self.path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|string| toml::from_str::<T>(string.as_str()).ok())
            .unwrap_or_else(T::default)
    }

    pub fn load_read_save<F, R>(&mut self, f: F) -> Result<Option<R>>
    where
        F: FnOnce(&mut T) -> R,
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

    pub fn load_and_save<F, R>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut T) -> R,
    {
        self.load_read_save(f).map(|_| ())
    }

    pub fn read<'a, F, R>(&'a self, f: F) -> Result<R>
    where
        F: FnOnce(&'a T) -> R,
    {
        Ok(f(self.state.get_or_init(|| self.load())))
    }
}
