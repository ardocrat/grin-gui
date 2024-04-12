use std::fs;
use std::path::PathBuf;

use grin_config::{config, GlobalConfig};
use grin_core::global;
use grin_servers as servers;
use grin_util::logger::LogEntry;
use servers::Server;

use futures::channel::oneshot;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use chrono::prelude::Utc;

use crate::logger;

pub use global::ChainTypes;

use iced_futures::futures::channel::mpsc as iced_mpsc;
use subscriber::UIMessage;

pub mod subscriber;

// Re-exports
pub use grin_chain::types::SyncStatus;
pub use grin_core::core::{amount_from_hr_string, amount_to_hr_string};
pub use grin_keychain::Identifier;
pub use grin_servers::ServerStats;

/// TODO - this differs from the default directory in 5.x,
/// need to reconcile this with existing installs somehow
const GRIN_HOME: &str = ".grin";

pub const GRIN_TOP_LEVEL_DIR: &str = "grin_node";

pub const GRIN_DEFAULT_DIR: &str = "default";

pub const SERVER_CONFIG_FILE_NAME: &str = "grin-server.toml";

/// Node Rest API and V2 Owner API secret
pub const API_SECRET_FILE_NAME: &str = ".api_secret";
/// Foreign API secret
pub const FOREIGN_API_SECRET_FILE_NAME: &str = ".foreign_api_secret";

fn get_grin_node_default_path(chain_type: &global::ChainTypes) -> PathBuf {
	// Check if grin dir exists
	let mut grin_path = match dirs::home_dir() {
		Some(p) => p,
		None => PathBuf::new(),
	};
	grin_path.push(GRIN_HOME);
	grin_path.push(chain_type.shortname());
	grin_path.push(GRIN_TOP_LEVEL_DIR);
	grin_path.push(GRIN_DEFAULT_DIR);

	if !grin_path.exists() {
		if let Err(e) = fs::create_dir_all(grin_path.clone()) {
			panic!("Unable to create default node path: {}", e);
		}
	}

	grin_path
}

// include build information
pub mod built_info {
	include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

pub fn info_strings() -> (String, String) {
	(
		format!(
			"This is Grin version {}{}, built for {} by {}.",
			built_info::PKG_VERSION,
			built_info::GIT_VERSION.map_or_else(|| "".to_owned(), |v| format!(" (git {})", v)),
			built_info::TARGET,
			built_info::RUSTC_VERSION,
		),
		format!(
			"Built with profile \"{}\", features \"{}\".",
			built_info::PROFILE,
			built_info::FEATURES_STR,
		),
	)
}

fn log_build_info() {
	let (basic_info, detailed_info) = info_strings();
	info!("{}", basic_info);
	debug!("{}", detailed_info);
}

fn log_feature_flags() {
	info!("Feature: NRD kernel enabled: {}", global::is_nrd_enabled());
}

pub struct Controller<'a> {
	logs_rx: mpsc::Receiver<LogEntry>,
	controller_rx: &'a mpsc::Receiver<ControllerMessage>,
	ui_tx: iced_mpsc::Sender<UIMessage>,
}

pub enum ControllerMessage {
	Shutdown,
}

/// This needs to provide the interface in to the server, bridging between the UI and
/// server instance
impl<'a> Controller<'a> {
	/// Create a new controller
	pub fn new(
		logs_rx: mpsc::Receiver<LogEntry>,
		ui_tx: iced_mpsc::Sender<UIMessage>,
		controller_rx: &'a mpsc::Receiver<ControllerMessage>,
	) -> Self {
		Self {
			logs_rx,
			controller_rx,
			ui_tx,
		}
	}

	/// Run the controller
	pub fn run(&mut self, server: Server, chain_type: global::ChainTypes) {
		let stat_update_interval = 1;
		let mut next_stat_update = Utc::now().timestamp() + stat_update_interval;
		let delay = Duration::from_millis(50);

		warn!("Running {:?}", chain_type);

		loop {
			if let Some(message) = self.controller_rx.try_iter().next() {
				match message {
					ControllerMessage::Shutdown => {
						warn!("Shutdown {:?} in progress, please wait", chain_type);
						// TODO this may hang on some errors
						server.stop();
						return;
					}
				}
			}

			if Utc::now().timestamp() > next_stat_update {
				next_stat_update = Utc::now().timestamp() + stat_update_interval;
				if let Ok(stats) = server.get_server_stats() {
					if let Err(e) = self.ui_tx.try_send(UIMessage::UpdateStatus(stats)) {
						error!("Unable to send stat message to UI: {}", e);
					}
				}
			}
			thread::sleep(delay);
		}
	}
}

pub struct NodeInterface {
	pub chain_type: Option<global::ChainTypes>,
	pub config: Option<GlobalConfig>,
	pub ui_sender: Option<iced_mpsc::Sender<UIMessage>>, //pub ui_rx: mpsc::Receiver<UIMessage>,
	pub node_started: bool,
	controller_tx: Option<mpsc::Sender<ControllerMessage>>,
	handle: Option<std::thread::JoinHandle<()>>,
}

impl NodeInterface {
	pub fn new() -> Self {
		NodeInterface {
			chain_type: None,
			config: None,
			ui_sender: None,
			node_started: false,
			controller_tx: None,
			handle: None,
		}
	}

	pub fn set_ui_sender(&mut self, ui_sender: iced_mpsc::Sender<UIMessage>) {
		self.ui_sender = Some(ui_sender)
	}

	/// Check that the api secret files exist and are valid
	fn check_api_secret_files(&self, chain_type: &global::ChainTypes, secret_file_name: &str) {
		let grin_path = get_grin_node_default_path(&chain_type);
		let mut api_secret_path = grin_path;
		api_secret_path.push(secret_file_name);
		if !api_secret_path.exists() {
			config::init_api_secret(&api_secret_path);
		} else {
			config::check_api_secret(&api_secret_path);
		}
	}

	fn load_or_create_default_config(&mut self, chain_type: global::ChainTypes) -> GlobalConfig {
		self.check_api_secret_files(&chain_type, API_SECRET_FILE_NAME);
		self.check_api_secret_files(&chain_type, FOREIGN_API_SECRET_FILE_NAME);

		let grin_path = get_grin_node_default_path(&chain_type);

		// Get path to default config file
		let mut config_path = grin_path.clone();
		config_path.push(SERVER_CONFIG_FILE_NAME);

		// Spit it out if it doesn't exist
		if !config_path.exists() {
			let mut default_config = GlobalConfig::for_chain(&chain_type);
			// update paths relative to current dir
			default_config.update_paths(&grin_path);
			if let Err(e) = default_config.write_to_file(config_path.to_str().unwrap()) {
				panic!("Unable to write default node config file: {}", e);
			}
		}

		GlobalConfig::new(config_path.to_str().unwrap()).unwrap()
	}

	pub fn shutdown_server(&mut self, join: bool) {
		if let Some(handle) = self.handle.take() {
			self.controller_tx
				.clone()
				.unwrap()
				.send(ControllerMessage::Shutdown);

			if join {
				handle.join().expect("could not join spawned thread");
			}

			self.node_started = false;
			self.controller_tx = None;
		}
	}

	pub fn restart_server(&mut self, chain_type: global::ChainTypes) {
		self.shutdown_server(false);
		self.start_server(chain_type);
	}

	pub fn start_server(&mut self, chain_type: global::ChainTypes) {
		self.chain_type = Some(chain_type);
		global::set_global_chain_type(chain_type);

		let node_config = self.load_or_create_default_config(chain_type);

		self.config = Some(node_config.clone());

		let config = node_config.clone();
		let mut logging_config = config.members.as_ref().unwrap().logging.clone().unwrap();
		logging_config.tui_running = Some(false);

		let api_chan: &'static mut (oneshot::Sender<()>, oneshot::Receiver<()>) =
			Box::leak(Box::new(oneshot::channel::<()>()));

		// TODO logs_tx needs to be used for something??
		let (_logs_tx, logs_rx) = {
			let (logs_tx, logs_rx) = mpsc::sync_channel::<LogEntry>(200);
			(Some(logs_tx), Some(logs_rx))
		};

		logger::update_logging_config(logger::LogArea::Node, logging_config);

		if let Some(file_path) = &config.config_file_path {
			info!(
				"Using configuration file at {}",
				file_path.to_str().unwrap()
			);
		} else {
			info!("Node configuration file not found, using default");
		};

		log_build_info();
		info!("Chain: {:?}", global::get_chain_type());
		match chain_type {
			ChainTypes::Mainnet => {
				// Set various mainnet specific feature flags.
				global::set_global_nrd_enabled(false);
			}
			_ => {
				// Set various non-mainnet feature flags.
				global::set_global_nrd_enabled(true);
			}
		}
		let afb = config
			.members
			.as_ref()
			.unwrap()
			.server
			.pool_config
			.accept_fee_base;
		global::set_global_accept_fee_base(afb);
		info!("Accept Fee Base: {:?}", global::get_accept_fee_base());
		global::set_global_future_time_limit(config.members.unwrap().server.future_time_limit);
		info!("Future Time Limit: {:?}", global::get_future_time_limit());
		log_feature_flags();

		let server_config = node_config.members.as_ref().unwrap().server.clone();

		let ui_sender = self.ui_sender.as_ref().unwrap().clone();
		self.node_started = true;

		let (controller_tx, controller_rx) = mpsc::channel::<ControllerMessage>();
		self.controller_tx = Some(controller_tx);

		let handle = thread::Builder::new()
			.name("node_runner".to_string())
			.spawn(move || {
				// TODO handle start up errors due to corrupt data
				servers::Server::start(
					server_config,
					logs_rx,
					|serv: servers::Server, logs_rx: Option<mpsc::Receiver<LogEntry>>| {
						let mut controller =
							Controller::new(logs_rx.unwrap(), ui_sender.clone(), &controller_rx);
						controller.run(serv, chain_type);
					},
					None,
					api_chan,
				);
			})
			.unwrap();

		self.handle = Some(handle);
	}
}
