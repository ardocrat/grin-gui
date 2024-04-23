use crate::error::FilesystemError;

use once_cell::sync::Lazy;
#[cfg(not(windows))]
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

pub mod backup;
mod save;
#[cfg(feature = "default")]
mod theme;

pub use save::PersistentData;
#[cfg(feature = "default")]
pub use theme::{import_theme, load_user_themes};

pub const GRINGUI_CONFIG_DIR: &str = ".grin-gui";

pub static CONFIG_DIR: Lazy<Mutex<PathBuf>> = Lazy::new(|| {
	// Returns the location of the config directory. Will create if it doesn't
	// exist.
	//
	// $HOME/.config/grin_gui
	#[cfg(not(windows))]
	{
		let home = env::var("HOME").expect("user home directory not found.");
		let config_dir = PathBuf::from(&home).join(&format!("{}/gui", GRINGUI_CONFIG_DIR));

		Mutex::new(config_dir)
	}

	// Returns the location of the config directory. Will create if it doesn't
	// exist.
	//
	// %HOME%\grin_gui
	#[cfg(windows)]
	{
		let config_dir = dirs_next::home_dir()
			.map(|path| path.join(GRINGUI_CONFIG_DIR))
			.map(|path| path.join("gui"))
			.expect("user home directory not found.");

		Mutex::new(config_dir)
	}
});

pub fn config_dir() -> PathBuf {
	let config_dir = CONFIG_DIR.lock().unwrap().clone();

	if !config_dir.exists() {
		let _ = fs::create_dir_all(&config_dir);
	}

	config_dir
}

type Result<T, E = FilesystemError> = std::result::Result<T, E>;
