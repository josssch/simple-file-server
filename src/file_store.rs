use std::{
    cell::RefCell,
    fs::File,
    io::{self, BufReader, Read},
    iter,
    path::{Path, PathBuf},
    rc::Rc,
};

use path_clean::PathClean;
use serde::{Deserialize, Serialize};

use crate::cache_map::CacheMap;

pub trait FileStore {
    type File: ServeableFile;

    fn exists(&self, path: impl AsRef<Path>) -> bool;
    fn get_file(&self, path: impl AsRef<Path>) -> Option<Rc<Self::File>>;
}

pub trait ServeableFile {
    fn name(&self) -> &str;
    fn metadata(&self) -> &FileMetadata;
    fn bytes_iter(&self) -> Box<dyn Iterator<Item = io::Result<Vec<u8>>> + 'static>;
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FileMetadata {
    hash: String,
    size_bytes: u64,
}

impl FileMetadata {
    pub fn hash(&self) -> &str {
        &self.hash
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }
}

// ------------------------

pub struct FsFileStore {
    base_path: PathBuf,
    cache: RefCell<CacheMap<PathBuf, Rc<FsFile>>>,
}

impl FsFileStore {
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        FsFileStore {
            base_path: base_path.as_ref().to_path_buf(),
            cache: RefCell::new(CacheMap::new()),
        }
    }

    fn full_path(&self, path: impl AsRef<Path>) -> Option<PathBuf> {
        // makes use of path_clean crate to clean up any .. or . segments
        // to prevent directory traversal attacks
        let combined = self.base_path.join(path).clean();

        // ensure the final cleaned path is still within base directory
        if combined.starts_with(&self.base_path) {
            Some(combined)
        } else {
            None
        }
    }
}

impl FileStore for FsFileStore {
    type File = FsFile;

    fn exists(&self, path: impl AsRef<Path>) -> bool {
        self.full_path(path).is_some_and(|p| p.is_file())
    }

    fn get_file(&self, path: impl AsRef<Path>) -> Option<Rc<Self::File>> {
        if !self.exists(&path) {
            return None;
        }

        let file_path = self.full_path(path)?;
        let mut cache = self.cache.borrow_mut();
        if let Some(file) = cache.get(&file_path) {
            return Some(file.clone());
        }

        let file = Rc::new(FsFile::new_existing(&file_path));
        cache.insert(file_path.clone(), Rc::clone(&file));

        Some(file)
    }
}

pub const METADATA_FILE_EXT: &str = ".metadata.json";

fn metadata_path(path: &PathBuf) -> PathBuf {
    let mut os_str = path
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_default();

    os_str.push(METADATA_FILE_EXT);
    path.with_file_name(os_str)
}

pub struct FsFile {
    path: PathBuf,
    metadata_path: PathBuf,
    metadata: FileMetadata,
}

impl FsFile {
    #[allow(unused)]
    pub fn create() -> Self {
        todo!()
    }

    pub fn new_existing(file_path: impl AsRef<Path>) -> Self {
        let path = file_path.as_ref().to_path_buf();
        let metadata_path = metadata_path(&path);

        let mut file = FsFile {
            path,
            metadata_path,
            metadata: FileMetadata::default(),
        };

        file.metadata = file.read_metadata().unwrap_or_default();
        file
    }

    fn read_metadata(&self) -> Result<FileMetadata, io::Error> {
        let metadata_file = File::open(&self.metadata_path)?;
        let metadata = serde_json::from_reader(metadata_file)?;
        Ok(metadata)
    }
}

impl ServeableFile for FsFile {
    fn name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|os_str| os_str.to_str())
            .unwrap_or("unknown")
    }

    fn metadata(&self) -> &FileMetadata {
        &self.metadata
    }

    fn bytes_iter(&self) -> Box<dyn Iterator<Item = Result<Vec<u8>, std::io::Error>> + 'static> {
        let file = File::open(&self.path).unwrap();

        let mut reader = BufReader::new(file);
        let mut buffer = [0; 8192];
        let mut is_failed = false;

        Box::new(iter::from_fn(move || {
            // stop iteration on failure, so we don't keep trying to read a broken stream
            if is_failed {
                return None;
            }

            let bytes_read = match reader.read(&mut buffer) {
                Ok(0) => return None, // EOF
                Ok(n) => n,
                Err(_) => {
                    // mark as failed, so we don't keep trying but so we can return an error once
                    is_failed = true;
                    return Some(Err(io::Error::new(
                        io::ErrorKind::Other,
                        "Failed to read file",
                    )));
                }
            };

            return Some(Ok(Vec::from(&buffer[..bytes_read])));
        }))
    }
}
