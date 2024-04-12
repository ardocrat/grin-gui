use grin_core::global::ChainTypes;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Struct for settings related to World of Warcraft.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Wallet {
	#[serde(default)]
	#[allow(deprecated)]
	/// Top-level directory. Should (but not always) contain grin_wallet.toml file
	pub tld: Option<PathBuf>,
	/// Display name in wallet selection
	pub display_name: String,
	/// If true, override the grin_wallet.toml configured node and use the internal one
	pub use_embedded_node: bool,
	/// Chain type of wallet
	pub chain_type: ChainTypes,
}

impl Wallet {
	pub fn new(tld: Option<PathBuf>, display_name: String, chain_type: ChainTypes) -> Self {
		Self {
			tld,
			display_name,
			use_embedded_node: true,
			chain_type,
		}
	}
}

impl Default for Wallet {
	fn default() -> Self {
		Wallet {
			tld: None,
			display_name: "Default".to_owned(),
			use_embedded_node: true,
			chain_type: ChainTypes::Mainnet,
		}
	}
}
