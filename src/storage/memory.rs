use std::{
    collections::HashMap,
    io::{self, Read},
    path::{Component, Path, PathBuf},
    sync::{Arc, RwLock},
    time::SystemTime,
};

use sha2::{Digest, Sha256};

use crate::storage::{
    FileMetadata, FileMetadataEntry, FileStore, StoredFile, fs::METADATA_FILE_EXT,
};

pub struct MemoryFileStore {
    files: RwLock<HashMap<PathBuf, Arc<MemoryFile>>>,
}

impl MemoryFileStore {
    pub fn new() -> Self {
        Self {
            files: RwLock::new(HashMap::new()),
        }
    }

    fn normalize_path(path: &Path) -> Option<PathBuf> {
        if path.is_absolute() {
            return None;
        }

        let mut normalized = PathBuf::new();

        for component in path.components() {
            match component {
                Component::Normal(part) => normalized.push(part),
                Component::CurDir => continue,
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
            }
        }

        if normalized.file_name().is_none() {
            return None;
        }

        Some(normalized)
    }

    fn is_valid_path(relative_path: &Path) -> bool {
        let name = match relative_path.file_name().and_then(|p| p.to_str()) {
            Some(name) => name.to_ascii_lowercase(),
            None => return false,
        };

        if name.ends_with(METADATA_FILE_EXT) {
            return false;
        }

        if let Some(Component::Normal(first_component)) = relative_path.components().next() {
            if let Some(first_component) = first_component.to_str() {
                return first_component != "api";
            }
        }

        false
    }
}

impl FileStore for MemoryFileStore {
    type File = MemoryFile;

    fn exists(&self, path: &Path) -> bool {
        let Some(path) = Self::normalize_path(path) else {
            return false;
        };

        if !Self::is_valid_path(&path) {
            return false;
        }

        let files = self.files.read().unwrap();
        files.contains_key(&path)
    }

    fn list(&self) -> io::Result<Vec<FileMetadataEntry>> {
        let files = self.files.read().unwrap();

        Ok(files
            .iter()
            .map(|(path, file)| FileMetadataEntry {
                path: path.to_string_lossy().to_string(),
                metadata: file.metadata().clone(),
            })
            .collect())
    }

    fn get_file(&self, path: &Path) -> Option<Arc<Self::File>> {
        let path = Self::normalize_path(path)?;
        if !Self::is_valid_path(&path) {
            return None;
        }

        let files = self.files.read().unwrap();
        files.get(&path).cloned()
    }

    fn upload(&self, path: &Path, reader: &mut dyn Read) -> io::Result<()> {
        let path = Self::normalize_path(path).ok_or(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provided file path is in an invalid place",
        ))?;

        if !Self::is_valid_path(&path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot upload due to invalid file name or path",
            ));
        }

        let mut digest = Sha256::new();
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 8192];

        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }

            let chunk = &buffer[..n];
            digest.update(chunk);
            bytes.extend_from_slice(chunk);
        }

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let metadata = FileMetadata {
            hash: FileMetadata::hash_to_hex(digest),
            size_bytes: bytes.len() as u64,
            created_at: Some(now),
            ..Default::default()
        };

        let file = Arc::new(MemoryFile {
            bytes: Arc::new(bytes),
            metadata,
        });

        let mut files = self.files.write().unwrap();
        files.insert(path, file);

        Ok(())
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        let path = Self::normalize_path(path).ok_or(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provided file path is in an invalid place",
        ))?;

        if !Self::is_valid_path(&path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot remove due to invalid file name or path",
            ));
        }

        let mut files = self.files.write().unwrap();
        files.remove(&path);
        Ok(())
    }
}

pub struct MemoryFile {
    bytes: Arc<Vec<u8>>,
    metadata: FileMetadata,
}

impl StoredFile for MemoryFile {
    fn metadata(&self) -> &FileMetadata {
        &self.metadata
    }

    fn open_reader(&self) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(ArcBytesReader::new(Arc::clone(&self.bytes))))
    }
}

struct ArcBytesReader {
    bytes: Arc<Vec<u8>>,
    offset_bytes: usize,
}

impl ArcBytesReader {
    fn new(bytes: Arc<Vec<u8>>) -> Self {
        Self {
            bytes,
            offset_bytes: 0,
        }
    }
}

impl Read for ArcBytesReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.offset_bytes >= self.bytes.len() {
            return Ok(0);
        }

        let bytes_left = self.bytes.len() - self.offset_bytes;
        let bytes_to_copy = bytes_left.min(buffer.len());
        let end_bytes = self.offset_bytes + bytes_to_copy;

        buffer[..bytes_to_copy].copy_from_slice(&self.bytes[self.offset_bytes..end_bytes]);
        self.offset_bytes = end_bytes;

        Ok(bytes_to_copy)
    }
}
