//! What the desktop launcher's files the app shares (`launcher/src/account.rs`,
//! `sources.rs`, `mojang.rs`) take from its backend.
use std::path::PathBuf;
use std::sync::OnceLock;

pub const VERSION: &str = super::BUILD;

static DATA: OnceLock<PathBuf> = OnceLock::new();

pub fn set_data_dir(dir: PathBuf) {
    let _ = DATA.set(dir);
}

/// The launcher's data folder: accounts, the downloaded game.
pub fn data_dir() -> PathBuf {
    DATA.get().cloned().unwrap_or_default()
}
