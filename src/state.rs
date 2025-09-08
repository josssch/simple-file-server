use std::{ops::Deref, path::PathBuf, sync::Arc};

use actix_web::web::Data;
use futures::lock::Mutex;

use crate::cache_map::CacheMap;

#[derive(Debug, Clone)]
pub struct CachedFileEntry {
    hash: String,
    bytes: SharedBytes,
}

impl CachedFileEntry {
    pub fn new(bytes: Vec<u8>) -> Self {
        CachedFileEntry {
            hash: sha256::digest(&bytes),
            bytes: SharedBytes::new(bytes),
        }
    }

    pub fn hash(&self) -> &str {
        &self.hash
    }

    pub fn bytes(&self) -> &SharedBytes {
        &self.bytes
    }
}

#[derive(Debug, Clone)]
pub struct SharedBytes(Arc<[u8]>);

impl SharedBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        SharedBytes(Arc::from(bytes))
    }
}

impl Deref for SharedBytes {
    type Target = Arc<[u8]>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub type FileCache = Data<Mutex<CacheMap<PathBuf, CachedFileEntry>>>;
