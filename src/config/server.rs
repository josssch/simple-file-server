use std::{io, sync::Arc};

use serde::{Deserialize, Serialize};
use serde_default::DefaultFromSerde;

use crate::{
    config::file::ConfigFile,
    storage::{DynFileStore, FsFileStore, MemoryFileStore},
};

pub const SERVER_CONFIG_NAME: &str = "config/server.json";
pub const DEFAULT_LOCAL_BASE_DIR: &str = "files";

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileSource {
    Local { base_dir: String },
    Memory,
}

pub fn build_file_store(config: &FileSource) -> io::Result<Arc<dyn DynFileStore>> {
    let store: Arc<dyn DynFileStore> = match config {
        FileSource::Local { base_dir } => Arc::new(FsFileStore::new(base_dir)),
        FileSource::Memory => Arc::new(MemoryFileStore::new()),
    };

    Ok(store)
}

impl Default for FileSource {
    fn default() -> Self {
        FileSource::Local {
            base_dir: DEFAULT_LOCAL_BASE_DIR.into(),
        }
    }
}

#[derive(DefaultFromSerde, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct MemoryCache {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_cache_time_secs")]
    pub cache_time_secs: u64,
    #[serde(default = "default_max_size_bytes")]
    pub max_size_bytes: u64,
    #[serde(default = "default_max_files_cached")]
    pub max_files_cached: usize,
}

const fn default_enabled() -> bool {
    true
}

const fn default_cache_time_secs() -> u64 {
    300 // 5 minutes
}

const fn default_max_size_bytes() -> u64 {
    10 * 1024 * 1024 // 10 MB
}

const fn default_max_files_cached() -> usize {
    100 // 100 files * ~10MB each = ~1GB max of cached files
}

#[derive(DefaultFromSerde, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "FileSource::default")]
    pub files_source: FileSource,
    pub memory_cache: MemoryCache,
}

impl ServerConfig {
    pub fn new_file() -> ConfigFile<Self> {
        ConfigFile::new(SERVER_CONFIG_NAME)
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

const fn default_port() -> u16 {
    3000
}
