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

use crate::{cache_map::CacheMap, config::server::FileSource};

pub trait FileStorageCore {
    fn exists(&self, path: &Path) -> bool;
    fn get_file(&self, path: &Path) -> Option<Arc<StoredFile>>;
    fn upload(&self, path: &Path, reader: BufReader<File>) -> io::Result<()>;
    fn remove(&self, path: &Path) -> io::Result<()>;
}

pub enum FileStore {
    Filesystem(FsFileStore),
}

impl FileStorageCore for FileStore {
    fn exists(&self, path: &Path) -> bool {
        match self {
            FileStore::Filesystem(fs_store) => fs_store.exists(path),
        }
    }

    fn get_file(&self, path: &Path) -> Option<Arc<StoredFile>> {
        match self {
            FileStore::Filesystem(fs_store) => fs_store.get_file(path),
        }
    }

    fn upload(&self, path: &Path, reader: BufReader<File>) -> io::Result<()> {
        match self {
            FileStore::Filesystem(fs_store) => fs_store.upload(path, reader),
        }
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        match self {
            FileStore::Filesystem(fs_store) => fs_store.remove(path),
        }
    }
}

impl From<&FileSource> for FileStore {
    fn from(value: &FileSource) -> Self {
        match value {
            FileSource::Local { base_dir } => FileStore::Filesystem(FsFileStore::new(base_dir)),
        }
    }
}

pub trait StoredFileCore {
    fn metadata(&self) -> &FileMetadata;
    fn bytes_iter(&self) -> Box<dyn Iterator<Item = io::Result<Vec<u8>>> + 'static>;
}

pub enum StoredFile {
    Filesystem(FsFile),
}

impl StoredFileCore for StoredFile {
    fn metadata(&self) -> &FileMetadata {
        match self {
            StoredFile::Filesystem(fs_file) => fs_file.metadata(),
        }
    }

    fn bytes_iter(&self) -> Box<dyn Iterator<Item = io::Result<Vec<u8>>> + 'static> {
        match self {
            StoredFile::Filesystem(fs_file) => fs_file.bytes_iter(),
        }
    }
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
    cache: Mutex<CacheMap<PathBuf, Arc<StoredFile>>>,
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
        if combined.starts_with(&self.base_path) && combined.file_name().is_some() {
            Some(combined)
        } else {
            None
        }
    }

    fn is_valid_path(&self, path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();

        let name = match path.file_name().and_then(|p| p.to_str()) {
            Some(name) => name.to_ascii_lowercase(),
            _ => return false,
        };

        // this relies on the assumption that METADATA_FILE_EXT is all lowercase
        if name.ends_with(METADATA_FILE_EXT) {
            return false;
        }

        // get where the /api path would be, resulting in path conflicts
        let api_path = self.full_path("api").unwrap();
        if path.starts_with(api_path) {
            return false;
        }

        true
    }
}

impl FileStorageCore for FsFileStore {
    fn exists(&self, path: &Path) -> bool {
        self.full_path(path).is_some_and(|p| p.is_file())
    }

    fn get_file(&self, path: &Path) -> Option<Arc<StoredFile>> {
        if !self.exists(path) {
            return None;
        }

        let file_path = self.full_path(path)?;
        let mut cache = self.cache.lock().unwrap();
        if let Some(file) = cache.get(&file_path) {
            return Some(file.clone());
        }

        let file = Arc::new(FsFile::new_existing(&file_path).into());
        cache.insert(file_path.clone(), Arc::clone(&file));

        Some(file)
    }

    fn upload(&self, path: &Path, mut reader: BufReader<File>) -> io::Result<()> {
        let path = self.full_path(path).ok_or(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provided file path is in an invalid place",
        ))?;

        if !self.is_valid_path(&path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot upload due to invalid file name or path",
            ));
        }

        // ensure parent directories exist, if any
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

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

    fn remove(&self, path: &Path) -> io::Result<()> {
        let path = self.full_path(path).ok_or(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provided file path is in an invalid place",
        ))?;

        if !self.is_valid_path(&path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot remove due to invalid file name or path",
            ));
        }

        if !self.exists(&path) {
            return Ok(());
        }

        fs::remove_file(&path)?;
        let metadata_path = metadata_path(&path);
        if metadata_path.is_file() {
            fs::remove_file(metadata_path)?;
        }

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

impl From<FsFile> for StoredFile {
    fn from(value: FsFile) -> Self {
        StoredFile::Filesystem(value)
    }
}

impl StoredFileCore for FsFile {
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
