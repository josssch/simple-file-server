use std::{
    fs::{self, File},
    io::{self, BufReader, Read, Write},
    iter,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use path_clean::PathClean;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cache_map::CacheMap;

pub trait FileStore {
    fn exists(&self, path: &Path) -> bool;
    fn get_file(&self, path: &Path) -> Option<Arc<dyn ServeableFile>>;
    fn upload(&self, path: &Path, reader: BufReader<File>) -> io::Result<()>;
}

pub trait ServeableFile {
    fn name(&self) -> &str;
    fn metadata(&self) -> &FileMetadata;
    fn bytes_iter(&self) -> Box<dyn Iterator<Item = io::Result<Vec<u8>>> + 'static>;
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FileMetadata {
    pub hash: String,
    pub size_bytes: u64,
}

impl FileMetadata {
    pub fn hash_to_hex(digest: Sha256) -> String {
        format!("{:x}", digest.finalize())
    }
}

// ------------------------

pub struct FsFileStore {
    base_path: PathBuf,
    cache: Mutex<CacheMap<PathBuf, Arc<FsFile>>>,
}

impl FsFileStore {
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        FsFileStore {
            base_path: base_path.as_ref().to_path_buf(),
            cache: Mutex::new(CacheMap::new()),
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
    fn exists(&self, path: &Path) -> bool {
        self.full_path(path).is_some_and(|p| p.is_file())
    }

    fn get_file(&self, path: &Path) -> Option<Arc<dyn ServeableFile>> {
        if !self.exists(path) {
            return None;
        }

        let file_path = self.full_path(path)?;
        let mut cache = self.cache.lock().unwrap();
        if let Some(file) = cache.get(&file_path) {
            return Some(file.clone());
        }

        let file = Arc::new(FsFile::new_existing(&file_path));
        cache.insert(file_path.clone(), Arc::clone(&file));

        Some(file)
    }

    fn upload(&self, path: &Path, mut reader: BufReader<File>) -> io::Result<()> {
        let path = self.full_path(path).ok_or(io::Error::new(
            io::ErrorKind::InvalidFilename,
            "provided file path is in an invalid place",
        ))?;

        let mut target_file = File::create(&path)?;

        let mut digest = Sha256::new();
        let mut buffer = [0u8; 8192];
        let mut written_bytes: u64 = 0;

        loop {
            let n = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(err) => {
                    // attempt to clean up partial file on error
                    let _ = fs::remove_file(&path);
                    return Err(err);
                }
            };

            written_bytes += n as u64;
            let bytes = &buffer[..n];

            if let Err(err) = target_file.write(bytes) {
                let _ = fs::remove_file(&path);
                return Err(err);
            }

            digest.update(bytes);
        }

        let hash = FileMetadata::hash_to_hex(digest);
        let metadata = FileMetadata {
            hash,
            size_bytes: written_bytes,
        };

        let metadata_path = metadata_path(&path);
        let metadata_file = File::create(&metadata_path)?;
        serde_json::to_writer(metadata_file, &metadata)?;

        Ok(())
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

    fn bytes_iter(&self) -> Box<dyn Iterator<Item = io::Result<Vec<u8>>> + 'static> {
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
