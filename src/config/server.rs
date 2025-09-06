use serde::{Deserialize, Serialize};
use serde_default::DefaultFromSerde;

use crate::config::file::ConfigFile;

pub const SERVER_CONFIG_NAME: &str = "config/server.json";

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileSource {
    Local { base_dir: String },
}

impl Default for FileSource {
    fn default() -> Self {
        FileSource::Local {
            base_dir: "files".into(),
        }
    }
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
}

impl ServerConfig {
    pub fn new_file() -> ConfigFile<Self> {
        ConfigFile::new(SERVER_CONFIG_NAME)
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    3000
}
