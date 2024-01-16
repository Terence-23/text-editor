use std::{fs, io, path::PathBuf, sync::Arc};

use clap::{self, ArgAction, Parser};
use directories::BaseDirs;
use parking_lot::{
    lock_api::{RwLockReadGuard, RwLockWriteGuard},
    RawRwLock, RwLock,
};
use serde::{Deserialize, Serialize};

#[derive(Parser)]
pub struct CmdConfig {
    /// the path to the edited file.
    // #[arg(default_value = "f.txt")]
    file: Option<PathBuf>,
    /// path to the config file
    #[arg(short, long, value_name = "FILE", default_value = BaseDirs::new()
                .unwrap()
                .config_dir()
                .to_path_buf()
                .join("te.toml")
                .as_os_str()
                .to_owned()
        )
        ]
    config: PathBuf,
    /// overwrites the selected config with default values
    #[arg(long, action=ArgAction::SetTrue)]
    generate_config: bool,
}

impl CmdConfig {
    pub fn check_actions(&self) -> io::Result<bool> {
        if self.generate_config {
            println!("{}", &self.config.display());
            if let Some(p) = self.config.ancestors().take(2).last() {
                println!("{}", p.display());
                fs::create_dir_all(p)?;
            }
            // let mut f = File::create(&self.config)?;
            println!("created file");
            std::fs::write(
                &self.config,
                toml::to_string(&FileConfig::default()).unwrap().as_bytes(),
            )?;

            return Ok(true);
        }

        Ok(false)
    }
}

#[derive(Deserialize, Serialize)]
struct FileConfig {
    tab_size: usize,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self { tab_size: 4 }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub file: Option<PathBuf>,
    pub config_path: PathBuf,
    pub tab_size: usize,
}
impl From<CmdConfig> for Config {
    fn from(value: CmdConfig) -> Self {
        let fc = toml::from_str(&fs::read_to_string(value.config.to_owned()).unwrap_or("".into()))
            .unwrap_or(FileConfig::default());
        Self::merge(value, fc)
    }
}
impl Config {
    fn merge(cmd: CmdConfig, f: FileConfig) -> Config {
        Self {
            file: cmd.file,
            config_path: cmd.config,
            tab_size: f.tab_size,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SharedConfig {
    config: Arc<RwLock<Config>>,
}
impl SharedConfig {
    pub fn new(conf: Config) -> Self {
        Self {
            config: Arc::new(RwLock::new(conf)),
        }
    }
    pub fn read(&self) -> RwLockReadGuard<RawRwLock, Config> {
        self.config.read()
    }
    pub fn write(&self) -> RwLockWriteGuard<RawRwLock, Config> {
        self.config.write()
    }
}
