use std::{
    io::{self, Read},
    path::Path,
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub trait FileStore: Send + Sync {
    type File: StoredFile;

    fn exists(&self, path: &Path) -> bool;
    fn list(&self) -> io::Result<Vec<FileMetadataEntry>>;
    fn get_file(&self, path: &Path) -> Option<Arc<Self::File>>;
    fn upload(&self, path: &Path, reader: &mut dyn Read) -> io::Result<()>;
    fn remove(&self, path: &Path) -> io::Result<()>;
}

pub trait DynFileStore: Send + Sync {
    fn exists(&self, path: &Path) -> bool;
    fn list(&self) -> io::Result<Vec<FileMetadataEntry>>;
    fn get_file(&self, path: &Path) -> Option<Arc<dyn StoredFile>>;
    fn upload(&self, path: &Path, reader: &mut dyn Read) -> io::Result<()>;
    fn remove(&self, path: &Path) -> io::Result<()>;
}

impl<T> DynFileStore for T
where
    T: FileStore,
    T::File: 'static,
{
    fn exists(&self, path: &Path) -> bool {
        self.exists(path)
    }

    fn list(&self) -> io::Result<Vec<FileMetadataEntry>> {
        self.list()
    }

    fn get_file(&self, path: &Path) -> Option<Arc<dyn StoredFile>> {
        self.get_file(path).map(|file| file as Arc<dyn StoredFile>)
    }

    fn upload(&self, path: &Path, reader: &mut dyn Read) -> io::Result<()> {
        self.upload(path, reader)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        self.remove(path)
    }
}

impl<T: FileStore> FileStore for Arc<T> {
    type File = T::File;

    fn exists(&self, path: &Path) -> bool {
        (**self).exists(path)
    }

    fn list(&self) -> io::Result<Vec<FileMetadataEntry>> {
        (**self).list()
    }

    fn get_file(&self, path: &Path) -> Option<Arc<Self::File>> {
        (**self).get_file(path)
    }

    fn upload(&self, path: &Path, reader: &mut dyn Read) -> io::Result<()> {
        (**self).upload(path, reader)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        (**self).remove(path)
    }
}

pub trait StoredFile: Send + Sync {
    fn metadata(&self) -> &FileMetadata;
    fn open_reader(&self) -> io::Result<Box<dyn Read + Send>>;
}

impl<T: StoredFile> StoredFile for Arc<T> {
    fn metadata(&self) -> &FileMetadata {
        (**self).metadata()
    }

    fn open_reader(&self) -> io::Result<Box<dyn Read + Send>> {
        (**self).open_reader()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FileMetadata {
    pub hash: String,
    pub size_bytes: u64,

    #[serde(default)]
    pub created_at: Option<u64>,

    #[serde(default, flatten)]
    pub extra: ExtraMetadata,
}

impl FileMetadata {
    pub fn hash_to_hex(digest: Sha256) -> String {
        format!("{:x}", digest.finalize())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ExtraMetadata {
    #[default]
    Standard,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileMetadataEntry {
    pub path: String,
    pub metadata: FileMetadata,
}
