use std::{
    fs::{self, File},
    io::{self, BufReader, Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

use path_clean::PathClean;
use sha2::{Digest, Sha256};

use crate::{
    cache_map::CacheMap,
    storage::{FileMetadata, FileMetadataEntry, FileStore, StoredFile},
};

pub struct FsFileStore {
    base_path: PathBuf,
    cache: Mutex<CacheMap<PathBuf, Arc<FsFile>>>,
}

struct ResolvedPath {
    full: PathBuf,
    relative: PathBuf,
}

impl FsFileStore {
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        FsFileStore {
            base_path: base_path.as_ref().to_path_buf(),
            cache: Mutex::new(CacheMap::new()),
        }
    }

    fn collect_metadata(&self, dir: &Path, entries: &mut Vec<FileMetadataEntry>) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;

            if file_type.is_dir() {
                self.collect_metadata(&path, entries)?;
                continue;
            }

            if !file_type.is_file() {
                continue;
            }

            let relative = match path.strip_prefix(&self.base_path) {
                Ok(relative) => relative,
                Err(_) => continue,
            };

            if !self.is_valid_path(relative) {
                continue;
            }

            let file = FsFile::new_existing(&path);
            entries.push(FileMetadataEntry {
                path: relative.to_string_lossy().to_string(),
                metadata: file.metadata().clone(),
            });
        }

        Ok(())
    }

    fn resolve_path(&self, path: impl AsRef<Path>) -> Option<ResolvedPath> {
        let path = path.as_ref();

        if path.is_absolute() {
            return None;
        }

        // makes use of path_clean crate to clean up any .. or . segments
        // to prevent directory traversal attacks
        let combined = self.base_path.join(path).clean();

        // ensure the final cleaned path is still within base directory
        if !combined.starts_with(&self.base_path) || combined.file_name().is_none() {
            return None;
        }

        let relative = combined.strip_prefix(&self.base_path).ok()?.to_path_buf();

        Some(ResolvedPath {
            full: combined,
            relative,
        })
    }

    fn is_valid_path(&self, relative_path: &Path) -> bool {
        let name = match relative_path.file_name().and_then(|p| p.to_str()) {
            Some(name) => name.to_ascii_lowercase(),
            _ => return false,
        };

        // this relies on the assumption that METADATA_FILE_EXT is all lowercase
        if name.ends_with(METADATA_FILE_EXT) {
            return false;
        }

        if let Some(Component::Normal(first_component)) = relative_path.components().next() {
            if let Some(first_component) = first_component.to_str() {
                if first_component == "api" {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }
}

impl FileStore for FsFileStore {
    type File = FsFile;

    fn exists(&self, path: &Path) -> bool {
        let resolved = match self.resolve_path(path) {
            Some(resolved) => resolved,
            None => return false,
        };

        if !self.is_valid_path(&resolved.relative) {
            return false;
        }

        resolved.full.is_file()
    }

    fn list(&self) -> io::Result<Vec<FileMetadataEntry>> {
        let mut entries = Vec::new();
        self.collect_metadata(&self.base_path, &mut entries)?;
        Ok(entries)
    }

    fn get_file(&self, path: &Path) -> Option<Arc<Self::File>> {
        let resolved = match self.resolve_path(path) {
            Some(resolved) => resolved,
            None => return None,
        };

        if !self.is_valid_path(&resolved.relative) {
            return None;
        }

        if !resolved.full.is_file() {
            return None;
        }

        let mut cache = self.cache.lock().unwrap();
        if let Some(file) = cache.get(&resolved.full) {
            return Some(file.clone());
        }

        let file = Arc::new(FsFile::new_existing(&resolved.full).into());
        cache.insert(resolved.full.clone(), Arc::clone(&file));

        Some(file)
    }

    fn upload(&self, path: &Path, reader: &mut dyn Read) -> io::Result<()> {
        let resolved = self.resolve_path(path).ok_or(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provided file path is in an invalid place",
        ))?;

        if !self.is_valid_path(&resolved.relative) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot upload due to invalid file name or path",
            ));
        }

        let path = resolved.full;

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

        // don't keep the file open longer than it needs to be
        drop(target_file);

        let hash = FileMetadata::hash_to_hex(digest);
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let metadata = FileMetadata {
            hash,
            size_bytes: written_bytes,
            created_at: Some(now),
            ..Default::default()
        };

        let metadata_path = metadata_path(&path);
        let metadata_tmp_path = metadata_path.with_extension("json.tmp");

        let metadata_write_result = (|| -> io::Result<()> {
            let mut metadata_file = File::create(&metadata_tmp_path)?;
            serde_json::to_writer(&mut metadata_file, &metadata)?;
            metadata_file.sync_all()?;

            if let Err(err) = fs::rename(&metadata_tmp_path, &metadata_path) {
                if metadata_path.is_file() {
                    fs::remove_file(&metadata_path)?;
                    fs::rename(&metadata_tmp_path, &metadata_path)?;
                } else {
                    return Err(err);
                }
            }

            Ok(())
        })();

        if let Err(err) = metadata_write_result {
            let _ = fs::remove_file(&metadata_tmp_path);
            let _ = fs::remove_file(&path);
            return Err(err);
        }

        let mut cache = self.cache.lock().unwrap();
        cache.remove(&path);

        Ok(())
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        let resolved = self.resolve_path(path).ok_or(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provided file path is in an invalid place",
        ))?;

        if !self.is_valid_path(&resolved.relative) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot remove due to invalid file name or path",
            ));
        }

        if !resolved.full.is_file() {
            return Ok(());
        }

        fs::remove_file(&resolved.full)?;
        let metadata_path = metadata_path(&resolved.full);
        if metadata_path.is_file() {
            fs::remove_file(&metadata_path)?;
        }

        let mut cache = self.cache.lock().unwrap();
        cache.remove(&resolved.full);

        Ok(())
    }
}

pub const METADATA_FILE_EXT: &str = ".metadata.json";

fn metadata_path(path: &Path) -> PathBuf {
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

impl StoredFile for FsFile {
    fn metadata(&self) -> &FileMetadata {
        &self.metadata
    }

    fn open_reader(&self) -> io::Result<Box<dyn Read + Send>> {
        let file = File::open(&self.path)?;
        Ok(Box::new(BufReader::new(file)))
    }
}
