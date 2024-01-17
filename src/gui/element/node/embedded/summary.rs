use chrono::prelude::Utc;

use iced_aw::Card;

const NANO_TO_MILLIS: f64 = 1.0 / 1_000_000.0;

use {
	super::super::super::{DEFAULT_FONT_SIZE, DEFAULT_SUB_HEADER_FONT_SIZE},
	crate::gui::{GrinGui, Message},
	crate::localization::localized_string,
	crate::Result,
	grin_gui_core::node::{ChainTypes, ServerStats, SyncStatus},
	grin_gui_core::theme::ColorPalette,
	grin_gui_core::theme::{Column, Container, Row, Scrollable, Text},
	iced::widget::{scrollable, Space},
	iced::{alignment, Alignment, Command, Length},
};

pub struct StateContainer {}

impl Default for StateContainer {
	fn default() -> Self {
		Self {}
	}
}

#[derive(Debug, Clone)]
pub enum LocalViewInteraction {}

pub fn handle_message<'a>(
	grin_gui: &mut GrinGui,
	message: LocalViewInteraction,
) -> Result<Command<Message>> {
	let stats = &grin_gui.node_state.embedded_state.server_stats;
	let state = &mut grin_gui.node_state.embedded_state.summary_state;
	/*match message {
	}*/
	Ok(Command::none())
}

// TODO: Localization
fn format_sync_status(sync_status: &SyncStatus) -> String {
	match sync_status {
		SyncStatus::Initial => "Initializing".to_owned(),
		SyncStatus::NoSync => "Running".to_owned(),
		SyncStatus::AwaitingPeers(_) => "Waiting for peers".to_owned(),
		SyncStatus::HeaderSync {
			sync_head,
			highest_height,
			..
		} => {
			let percent = if *highest_height == 0 {
				0
			} else {
				sync_head.height * 100 / highest_height
			};
			format!("Sync step 1/7: Downloading headers: {}%", percent)
		}
		SyncStatus::TxHashsetPibd {
			aborted: _,
			errored: _,
			completed_leaves,
			leaves_required,
			completed_to_height: _,
			required_height: _,
		} => {
			let percent = if *completed_leaves == 0 {
				0
			} else {
				completed_leaves * 100 / leaves_required
			};
			format!(
				"Sync step 2/7: Downloading Tx state (PIBD) - {} / {} entries - {}%",
				completed_leaves, leaves_required, percent
			)
		}
		SyncStatus::TxHashsetDownload(stat) => {
			if stat.total_size > 0 {
				let percent = stat.downloaded_size * 100 / stat.total_size;
				let start = stat.prev_update_time.timestamp_nanos();
				let fin = Utc::now().timestamp_nanos();
				let dur_ms = (fin - start) as f64 * NANO_TO_MILLIS;

				format!("Sync step 2/7: Downloading {}(MB) chain state for state sync: {}% at {:.1?}(kB/s)",
							stat.total_size / 1_000_000,
							percent,
							if dur_ms > 1.0f64 { stat.downloaded_size.saturating_sub(stat.prev_downloaded_size) as f64 / dur_ms as f64 } else { 0f64 },
					)
			} else {
				let start = stat.start_time.timestamp_millis();
				let fin = Utc::now().timestamp_millis();
				let dur_secs = (fin - start) / 1000;

				format!("Sync step 2/7: Downloading chain state for state sync. Waiting remote peer to start: {}s",
							dur_secs,
					)
			}
		}
		SyncStatus::TxHashsetSetup {
			headers,
			headers_total,
			kernel_pos,
			kernel_pos_total,
		} => {
			if headers.is_some() && headers_total.is_some() {
				let h = headers.unwrap();
				let ht = headers_total.unwrap();
				let percent = h * 100 / ht;
				format!(
					"Sync step 3/7: Preparing for validation (kernel history) - {}/{} - {}%",
					h, ht, percent
				)
			} else if kernel_pos.is_some() && kernel_pos_total.is_some() {
				let k = kernel_pos.unwrap();
				let kt = kernel_pos_total.unwrap();
				let percent = k * 100 / kt;
				format!(
					"Sync step 3/7: Preparing for validation (kernel position) - {}/{} - {}%",
					k, kt, percent
				)
			} else {
				format!("Sync step 3/7: Preparing chain state for validation")
			}
		}
		SyncStatus::TxHashsetRangeProofsValidation {
			rproofs,
			rproofs_total,
		} => {
			let r_percent = if *rproofs_total > 0 {
				(rproofs * 100) / rproofs_total
			} else {
				0
			};
			format!(
				"Sync step 4/7: Validating chain state - range proofs: {}%",
				r_percent
			)
		}
		SyncStatus::TxHashsetKernelsValidation {
			kernels,
			kernels_total,
		} => {
			let k_percent = if *kernels_total > 0 {
				(kernels * 100) / kernels_total
			} else {
				0
			};
			format!(
				"Sync step 5/7: Validating chain state - kernels: {}%",
				k_percent
			)
		}
		SyncStatus::TxHashsetSave => {
			"Sync step 6/7: Finalizing chain state for state sync".to_owned()
		}
		SyncStatus::TxHashsetDone => {
			"Sync step 6/7: Finalized chain state for state sync".to_owned()
		}
		SyncStatus::BodySync {
			current_height,
			highest_height,
		} => {
			let percent = if *highest_height == 0 {
				0
			} else {
				current_height * 100 / highest_height
			};
			format!("Sync step 7/7: Downloading blocks: {}%", percent)
		}
		SyncStatus::Shutdown => "Shutting down, closing connections".to_owned(),
	}
}

pub fn data_container<'a>(
	state: &'a StateContainer,
	stats: &'a Option<ServerStats>,
	chain_type: ChainTypes,
) -> Container<'a, Message> {
	fn stat_row<'a>(label_text: String, value_text: String) -> Column<'a, Message> {
		let line_label = Text::new(label_text).size(DEFAULT_FONT_SIZE);

		let line_label_container = Container::new(line_label)
			.style(grin_gui_core::theme::ContainerStyle::NormalBackground);

		let line_value = Text::new(value_text).size(DEFAULT_FONT_SIZE);

		let line_value_container = Container::new(line_value)
			.style(grin_gui_core::theme::ContainerStyle::NormalBackground);

		Column::new()
			.push(line_label_container)
			.push(Space::new(Length::Fill, Length::Fixed(2.0)))
			.push(line_value_container)
			.push(Space::new(Length::Fill, Length::Fixed(10.0)))
			.align_items(Alignment::Center)
	}
	// Basic Info "Box"
	let stats_info_container = match stats {
		Some(s) => {
			let status_line_value = Text::new(format_sync_status(&s.sync_status))
				.size(DEFAULT_FONT_SIZE)
				.horizontal_alignment(alignment::Horizontal::Center);

			let status_line_value_container = Container::new(status_line_value)
				.style(grin_gui_core::theme::ContainerStyle::NormalBackground);

			let status_line_column = Column::new()
				.push(status_line_value_container)
				.align_items(Alignment::Center);

			let status_line_row = Row::new()
				.push(Space::new(Length::Fill, Length::Fixed(0.0)))
				.push(status_line_column)
				.push(Space::new(Length::Fill, Length::Fixed(0.0)))
				.align_items(Alignment::Center);

			let status_line_title = match chain_type {
				ChainTypes::Testnet => localized_string("status-line-title-test"),
				_ => localized_string("status-line-title-main"),
			};
			let status_line_container =
				Container::new(Text::new(status_line_title).size(DEFAULT_SUB_HEADER_FONT_SIZE))
					.width(Length::Fill)
					.center_x();

			let status_line_card = Card::new(status_line_container, status_line_row)
				.style(grin_gui_core::theme::CardStyle::Normal);

			// Basic status
			let connected_peers_row = stat_row(
				localized_string("connected-peers-label"),
				format!("{}", &s.peer_count),
			);
			let disk_usage_row = stat_row(
				localized_string("disk-usage-label"),
				format!("{}", &s.disk_usage_gb),
			);
			let basic_status_column = Column::new().push(connected_peers_row).push(disk_usage_row);

			let basic_status_container = Container::new(
				Text::new(localized_string("basic-status-title"))
					.size(DEFAULT_SUB_HEADER_FONT_SIZE),
			)
			.width(Length::Fill)
			.center_x();

			let basic_status_card = Card::new(basic_status_container, basic_status_column)
				.style(grin_gui_core::theme::CardStyle::Normal);

			// Tip Status
			let header_tip_hash_row = stat_row(
				localized_string("header-tip-label"),
				format!("{}", &s.header_stats.last_block_h),
			);
			let header_chain_height_row = stat_row(
				localized_string("header-chain-height-label"),
				format!("{}", &s.header_stats.height),
			);
			let header_chain_difficulty_row = stat_row(
				localized_string("header-chain-difficulty-label"),
				format!("{}", &s.header_stats.total_difficulty),
			);
			let header_tip_timestamp_row = stat_row(
				localized_string("header-tip-timestamp-label"),
				format!("{}", &s.header_stats.latest_timestamp),
			);
			let header_status_column = Column::new()
				.push(header_tip_hash_row)
				.push(header_chain_height_row)
				.push(header_chain_difficulty_row)
				.push(header_tip_timestamp_row);

			let header_status_container = Container::new(
				Text::new(localized_string("header-status-title"))
					.size(DEFAULT_SUB_HEADER_FONT_SIZE),
			)
			.width(Length::Fill)
			.center_x();

			let header_status_card = Card::new(header_status_container, header_status_column)
				.style(grin_gui_core::theme::CardStyle::Normal);

			// Chain status
			let chain_tip_hash_row = stat_row(
				localized_string("chain-tip-label"),
				format!("{}", &s.chain_stats.last_block_h),
			);
			let chain_height_row = stat_row(
				localized_string("chain-height-label"),
				format!("{}", &s.chain_stats.height),
			);
			let chain_difficulty_row = stat_row(
				localized_string("chain-difficulty-label"),
				format!("{}", &s.chain_stats.total_difficulty),
			);
			let chain_tip_timestamp_row = stat_row(
				localized_string("chain-tip-timestamp-label"),
				format!("{}", &s.chain_stats.latest_timestamp),
			);
			let chain_status_column = Column::new()
				.push(chain_tip_hash_row)
				.push(chain_height_row)
				.push(chain_difficulty_row)
				.push(chain_tip_timestamp_row);

			let chain_status_container = Container::new(
				Text::new(localized_string("chain-status-title"))
					.size(DEFAULT_SUB_HEADER_FONT_SIZE),
			)
			.width(Length::Fill)
			.center_x();

			let chain_status_card = Card::new(chain_status_container, chain_status_column)
				.style(grin_gui_core::theme::CardStyle::Normal);

			// TX Pool
			let tx_status_card = match &s.tx_stats {
				Some(t) => {
					let transaction_pool_size_row = stat_row(
						localized_string("transaction-pool-size-label"),
						format!("{}", t.tx_pool_size),
					);
					let stem_pool_size_row = stat_row(
						localized_string("stem-pool-size-label"),
						format!("{}", t.stem_pool_size),
					);
					let tx_status_column = Column::new()
						.push(transaction_pool_size_row)
						.push(stem_pool_size_row);

					let tx_status_container = Container::new(
						Text::new(localized_string("transaction-pool-title"))
							.size(DEFAULT_SUB_HEADER_FONT_SIZE),
					)
					.width(Length::Fill)
					.center_x();

					Card::new(tx_status_container, tx_status_column)
				}
				None => Card::new(
					Text::new(localized_string("transaction-pool-title")),
					Column::new(),
				),
			}
			.style(grin_gui_core::theme::CardStyle::Normal);

			let display_row_1 = Row::new()
				.push(status_line_card)
				.padding(iced::Padding::from([
					0, // top
					0, // right
					6, // bottom
					0, // left
				]))
				.spacing(10);

			let display_row_2 = Row::new()
				.push(header_status_card)
				.push(chain_status_card)
				.padding(iced::Padding::from([
					6, // top
					0, // right
					6, // bottom
					0, // left
				]))
				.spacing(10);
			let display_row_3 = Row::new()
				.push(basic_status_card)
				.push(tx_status_card)
				.padding(iced::Padding::from([
					6, // top
					0, // right
					0, // bottom
					0, // left
				]))
				.spacing(10);

			let status_column = Column::new()
				.push(display_row_1)
				.push(display_row_2)
				.push(display_row_3);

			Container::new(status_column)
		}
		None => Container::new(Column::new()),
	};

	let stats_info_container = stats_info_container.width(Length::Fixed(600.0));
	let scrollable = Scrollable::new(stats_info_container)
		//.align_items(Alignment::Center)
		.height(Length::Fill)
		//.width(Length::Fill)
		.style(grin_gui_core::theme::ScrollableStyle::Primary);

	Container::new(scrollable)
		.center_y()
		.center_x()
		.width(Length::Fill)
		.height(Length::Shrink)
}
