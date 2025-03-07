use sled::Db;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Context {
    pub local_server: bool,
    pub container_manager: Option<String>,
    pub container_id: Option<String>,
    pub output_dir: PathBuf,
    pub fav_dir: PathBuf,
    pub db: Db,
}
