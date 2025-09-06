use std::{
    fs::{self, File},
    io,
    path::PathBuf,
};

use serde::{Serialize, de::DeserializeOwned};

pub struct ConfigFile<T: DeserializeOwned + Serialize + Default> {
    file_path: PathBuf,

    has_been_read: bool,
    data: Option<T>,
}

impl<T: DeserializeOwned + Serialize + Default> ConfigFile<T> {
    pub fn new(file_path: impl Into<PathBuf>) -> Self {
        ConfigFile {
            file_path: file_path.into(),
            data: None,
            has_been_read: false,
        }
    }

    #[allow(unused)]
    pub fn get(&self) -> Option<&T> {
        self.data.as_ref()
    }

    pub fn take(self) -> Option<T> {
        self.data
    }

    pub fn read_from_file(&mut self) -> io::Result<&T> {
        if self.has_been_read {
            return Ok(self.data.as_ref().unwrap());
        }

        if !self.file_path.is_file() {
            self.write_default(false)?;
        }

        let file = File::open(&self.file_path)?;
        let data = serde_json::from_reader(&file)?;

        self.data = Some(data);
        self.has_been_read = true;

        Ok(self.data.as_ref().unwrap())
    }

    pub fn write_default(&mut self, force: bool) -> io::Result<()> {
        if self.file_path.is_file() && !force {
            return Ok(());
        }

        self.mkdirs()?;
        let file = File::create(&self.file_path)?;

        let default_data = T::default();
        serde_json::to_writer_pretty(&file, &default_data)?;
        self.data = Some(default_data);

        Ok(())
    }

    fn mkdirs(&self) -> io::Result<()> {
        let Some(parent) = self.file_path.parent() else {
            return Ok(());
        };

        fs::create_dir_all(parent)
    }
}
