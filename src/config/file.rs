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

    pub fn read(&mut self) -> io::Result<&T> {
        if self.has_been_read {
            return Ok(self.data.as_ref().unwrap());
        }

        if !self.file_path.is_file() {
            self.defaulted_and_save(false)?;
            return Ok(self.data.as_ref().unwrap());
        }

        let file = File::open(&self.file_path)?;
        let data = serde_json::from_reader(&file)?;

        self.data = Some(data);
        self.has_been_read = true;

        Ok(self.data.as_ref().unwrap())
    }

    pub fn read_and_save(&mut self) -> io::Result<()> {
        self.read()?;
        // re-save to ensure formatting and any new default fields
        self.save()?;
        Ok(())
    }

    pub fn defaulted_and_save(&mut self, force: bool) -> io::Result<()> {
        if self.file_path.is_file() && !force {
            return Ok(());
        }

        self.data = Some(T::default());
        self.save()?;

        Ok(())
    }

    pub fn save(&self) -> io::Result<()> {
        let Some(data) = &self.data else {
            return Ok(());
        };

        self.mkdirs()?;

        let file = File::create(&self.file_path)?;
        serde_json::to_writer_pretty(&file, data)?;

        Ok(())
    }

    fn mkdirs(&self) -> io::Result<()> {
        let Some(parent) = self.file_path.parent() else {
            return Ok(());
        };

        fs::create_dir_all(parent)
    }
}
