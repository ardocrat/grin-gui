use crate::config::SelfUpdateChannel;
use crate::error::DownloadError;
#[cfg(target_os = "macos")]
use crate::error::FilesystemError;
use crate::network::download_file;

use regex::Regex;
use retry::delay::Fibonacci;
use retry::{retry, Error as RetryError, OperationResult};
use serde::Deserialize;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Takes a `&str` and formats it into a proper
/// World of Warcraft release version.
///
/// Eg. 90001 would be 9.0.1.
pub fn format_interface_into_game_version(interface: &str) -> String {
	if interface.len() == 5 {
		let major = interface[..1].parse::<u8>();
		let minor = interface[1..3].parse::<u8>();
		let patch = interface[3..5].parse::<u8>();
		if let (Ok(major), Ok(minor), Ok(patch)) = (major, minor, patch) {
			return format!("{}.{}.{}", major, minor, patch);
		}
	}

	interface.to_owned()
}

/// Takes a `&str` and strips any non-digit.
/// This is used to unify and compare addon versions:
///
/// A string looking like 213r323 would return 213323.
/// A string looking like Rematch_4_10_15.zip would return 41015.
pub(crate) fn _strip_non_digits(string: &str) -> String {
	let re = Regex::new(r"[\D]").unwrap();
	let stripped = re.replace_all(string, "").to_string();
	stripped
}

#[derive(Debug, Deserialize, Clone)]
pub struct Release {
	pub tag_name: String,
	pub prerelease: bool,
	pub assets: Vec<ReleaseAsset>,
	pub body: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ReleaseAsset {
	pub name: String,
	#[serde(rename = "browser_download_url")]
	pub download_url: String,
}

#[cfg(feature = "no-self-update")]
pub async fn get_latest_release(_channel: SelfUpdateChannel) -> Option<Release> {
	None
}

#[cfg(not(feature = "no-self-update"))]
pub async fn get_latest_release(channel: SelfUpdateChannel) -> Option<Release> {
	use crate::network::request_async;
	use isahc::AsyncReadResponseExt;

	log::debug!("checking for application update");

	let mut resp = request_async(
		"https://api.github.com/repos/grin-gui/grin-gui/releases",
		vec![],
		None,
	)
	.await
	.ok()?;

	let releases: Vec<Release> = resp.json().await.ok()?;

	releases.into_iter().find(|r| {
		if channel == SelfUpdateChannel::Beta {
			// If beta, always want latest release
			true
		} else {
			// Otherwise ONLY non-prereleases
			!r.prerelease
		}
	})
}

/// Downloads the latest release file that matches `bin_name`, renames the current
/// executable to a temp path, renames the new version as the original file name,
/// then returns both the original file name (new version) and temp path (old version)
pub async fn download_update_to_temp_file(
	bin_name: String,
	release: Release,
) -> Result<(PathBuf, PathBuf), DownloadError> {
	#[cfg(not(target_os = "linux"))]
	let current_bin_path = std::env::current_exe()?;

	#[cfg(target_os = "linux")]
	let current_bin_path = PathBuf::from(
		std::env::var("APPIMAGE").map_err(|_| DownloadError::SelfUpdateLinuxNonAppImage)?,
	);

	// Path to download the new version to
	let download_path = current_bin_path
		.parent()
		.unwrap()
		.join(&format!("tmp_{}", bin_name));

	// Path to temporarily force rename current process to, se we can then
	// rename `download_path` to `current_bin_path` and then launch new version
	// cleanly as `current_bin_path`
	let tmp_path = current_bin_path
		.parent()
		.unwrap()
		.join(&format!("tmp2_{}", bin_name));

	// On macos, we actually download an archive with the new binary inside. Let's extract
	// that file and remove the archive.
	#[cfg(target_os = "macos")]
	{
		let asset_name = format!("{}-macos.tar.gz", bin_name);

		let asset = release
			.assets
			.iter()
			.find(|a| a.name == asset_name)
			.cloned()
			.ok_or(DownloadError::MissingSelfUpdateRelease { bin_name })?;

		let archive_path = current_bin_path.parent().unwrap().join(&asset_name);

		download_file(&asset.download_url, &archive_path).await?;

		extract_binary_from_tar(&archive_path, &download_path, "grin-gui")?;

		std::fs::remove_file(&archive_path)?;
	}

	// For windows & linux, we download the new binary directly
	#[cfg(not(target_os = "macos"))]
	{
		let asset = release
			.assets
			.iter()
			.find(|a| a.name == bin_name)
			.cloned()
			.ok_or(DownloadError::MissingSelfUpdateRelease { bin_name })?;

		download_file(&asset.download_url, &download_path).await?;
	}

	// Make executable
	#[cfg(not(target_os = "windows"))]
	{
		use async_std::fs;
		use std::os::unix::fs::PermissionsExt;

		let mut permissions = fs::metadata(&download_path).await?.permissions();
		permissions.set_mode(0o755);
		fs::set_permissions(&download_path, permissions).await?;
	}

	rename(&current_bin_path, &tmp_path)?;

	rename(&download_path, &current_bin_path)?;

	Ok((current_bin_path, tmp_path))
}

/// Extracts the Grin Gui binary from a `tar.gz` archive to temp_file path
#[cfg(target_os = "macos")]
fn extract_binary_from_tar(
	archive_path: &Path,
	temp_file: &Path,
	bin_name: &str,
) -> Result<(), FilesystemError> {
	use flate2::read::GzDecoder;
	use std::fs::File;
	use std::io::copy;
	use tar::Archive;

	let mut archive = Archive::new(GzDecoder::new(File::open(&archive_path)?));

	let mut temp_file = File::create(temp_file)?;

	for file in archive.entries()? {
		let mut file = file?;

		let path = file.path()?;

		if let Some(name) = path.to_str() {
			if name == bin_name {
				copy(&mut file, &mut temp_file)?;

				return Ok(());
			}
		}
	}

	Err(FilesystemError::BinMissingFromTar {
		bin_name: bin_name.to_owned(),
	})
}

/// Rename a file or directory to a new name, retrying if the operation fails because of permissions
///
/// Will retry for ~30 seconds with longer and longer delays between each, to allow for virus scan
/// and other automated operations to complete.
pub fn rename<F, T>(from: F, to: T) -> io::Result<()>
where
	F: AsRef<Path>,
	T: AsRef<Path>,
{
	// 21 Fibonacci steps starting at 1 ms is ~28 seconds total
	// See https://github.com/rust-lang/rustup/pull/1873 where this was used by Rustup to work around
	// virus scanning file locks
	let from = from.as_ref();
	let to = to.as_ref();

	retry(Fibonacci::from_millis(1).take(21), || {
		match fs::rename(from, to) {
			Ok(_) => OperationResult::Ok(()),
			Err(e) => match e.kind() {
				io::ErrorKind::PermissionDenied => OperationResult::Retry(e),
				_ => OperationResult::Err(e),
			},
		}
	})
	.map_err(|e| match e {
		RetryError::Operation { error, .. } => error,
		RetryError::Internal(message) => io::Error::new(io::ErrorKind::Other, message),
	})
}

/// Remove a file, retrying if the operation fails because of permissions
///
/// Will retry for ~30 seconds with longer and longer delays between each, to allow for virus scan
/// and other automated operations to complete.
pub fn remove_file<P>(path: P) -> io::Result<()>
where
	P: AsRef<Path>,
{
	// 21 Fibonacci steps starting at 1 ms is ~28 seconds total
	// See https://github.com/rust-lang/rustup/pull/1873 where this was used by Rustup to work around
	// virus scanning file locks
	let path = path.as_ref();

	retry(
		Fibonacci::from_millis(1).take(21),
		|| match fs::remove_file(path) {
			Ok(_) => OperationResult::Ok(()),
			Err(e) => match e.kind() {
				io::ErrorKind::PermissionDenied => OperationResult::Retry(e),
				_ => OperationResult::Err(e),
			},
		},
	)
	.map_err(|e| match e {
		RetryError::Operation { error, .. } => error,
		RetryError::Internal(message) => io::Error::new(io::ErrorKind::Other, message),
	})
}

pub(crate) fn _truncate(s: &str, max_chars: usize) -> &str {
	match s.char_indices().nth(max_chars) {
		None => s,
		Some((idx, _)) => &s[..idx],
	}
}

pub(crate) fn _regex_html_tags_to_newline() -> Regex {
	regex::Regex::new(r"<br ?/?>|#.\s").unwrap()
}

pub(crate) fn _regex_html_tags_to_space() -> Regex {
	regex::Regex::new(r"<[^>]*>|&#?\w+;|[gl]t;").unwrap()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_interface() {
		let interface = "90001";
		assert_eq!("9.0.1", format_interface_into_game_version(interface));

		let interface = "11305";
		assert_eq!("1.13.5", format_interface_into_game_version(interface));

		let interface = "100000";
		assert_eq!("100000", format_interface_into_game_version(interface));

		let interface = "9.0.1";
		assert_eq!("9.0.1", format_interface_into_game_version(interface));
	}
}
