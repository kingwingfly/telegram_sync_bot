use core::fmt;
use sled::Db;
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, RwLock},
};
use teloxide::types::UserId;

#[derive(Debug, Clone)]
pub struct Context {
    pub local_server: bool,
    pub container_manager: Option<String>,
    pub container_id: Option<String>,

    pub bypass_pwd: Arc<RwLock<String>>,
    pub bypass_users: HashSet<UserId>,

    // channel only: score >= fav_score_limit will be fav
    pub fav_score_limit: i32,
    // channel only: score < delete_score_limit will be deleted
    pub delete_score_limit: i32,

    pub output_dir: PathBuf,
    pub fav_dir: PathBuf,
    pub trash_dir: PathBuf,

    pub db: Db,
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Context: ")?;
        write!(f, "local_server: {}, ", self.local_server)?;
        write!(f, "container_manager: {:?}, ", self.container_manager)?;
        write!(f, "container_id: {:?}, ", self.container_id)?;
        write!(f, "by_pass_pwd: {}, ", self.bypass_pwd.read().unwrap())?;
        write!(f, "bypass users: {:?}, ", self.bypass_users)?;
        write!(f, "output_dir: {:?}, ", self.output_dir)?;
        write!(f, "fav_dir: {:?}", self.fav_dir)
    }
}
