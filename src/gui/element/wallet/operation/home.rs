use super::tx_list::{self, ExpandType};
use async_std::prelude::FutureExt;
use grin_gui_core::{
    config::Config,
    wallet::{TxLogEntry, TxLogEntryType},
};
use iced_aw::Card;
use iced_native::Widget;
use std::path::PathBuf;

use super::action_menu;
use super::tx_list::{HeaderState, TxList, TxLogEntryWrap};
use grin_gui_widgets::widget::header;

use {
    super::super::super::{
        DEFAULT_FONT_SIZE, DEFAULT_HEADER_FONT_SIZE, DEFAULT_PADDING, DEFAULT_SUB_HEADER_FONT_SIZE,
        SMALLER_FONT_SIZE,
    },
    crate::gui::{GrinGui, Interaction, Message},
    crate::localization::localized_string,
    crate::log_error,
    crate::Result,
    anyhow::Context,
    grin_gui_core::theme::{
        Button, Column, Container, Element, Header, PickList, Row, Scrollable, TableRow, Text,
        TextInput,
    },
    grin_gui_core::wallet::{StatusMessage, WalletInfo, WalletInterface},
    grin_gui_core::{node::amount_to_hr_string, theme::ColorPalette},
    iced::widget::{button, pick_list, scrollable, text_input, Checkbox, Space},
    iced::{alignment, Alignment, Command, Length},
    std::sync::{Arc, RwLock},
};

pub struct StateContainer {
    pub action_menu_state: action_menu::StateContainer,
    pub expanded_type: ExpandType,

    wallet_info: Option<WalletInfo>,
    wallet_txs: TxList,
    wallet_status: String,
    // txs_scrollable_state: scrollable::State,
    last_summary_update: chrono::DateTime<chrono::Local>,
    tx_header_state: HeaderState,

    // chart
    chart: super::chart::CpuUsageChart,
}

impl Default for StateContainer {
    fn default() -> Self {
        let now = chrono::Utc::now();
        let chart = super::chart::CpuUsageChart::new(
            vec![
                (now, 100),
                (now, 100),
                (now, 100),
                (now, 100),
                (now, 100),
                (now, 100),
                (now, 100),
                (now, 100),
            ]
            .into_iter(),
        );

        Self {
            action_menu_state: Default::default(),
            // back_button_state: Default::default(),
            expanded_type: ExpandType::None,
            wallet_info: Default::default(),
            wallet_txs: Default::default(),
            wallet_status: Default::default(),
            // txs_scrollable_state: Default::default(),
            last_summary_update: Default::default(),
            tx_header_state: Default::default(),
            chart,
        }
    }
}

#[derive(Debug, Clone)]
pub enum LocalViewInteraction {
    Back,
    Submit,
    /// was updated from node, info
    WalletInfoUpdateSuccess(bool, WalletInfo, Vec<TxLogEntry>),
    WalletInfoUpdateFailure(Arc<RwLock<Option<anyhow::Error>>>),
    WalletSlatepackAddressUpdateSuccess(String),
    WalletCloseError(Arc<RwLock<Option<anyhow::Error>>>),
    WalletCloseSuccess,
    CancelTx(u32),
    TxCancelledOk(u32),
    TxCancelError(Arc<RwLock<Option<anyhow::Error>>>),
}

// Okay to modify state and access wallet here
pub fn handle_tick<'a>(
    grin_gui: &mut GrinGui,
    time: chrono::DateTime<chrono::Local>,
) -> Result<Command<Message>> {
    {
        let w = grin_gui.wallet_interface.read().unwrap();
        if !w.wallet_is_open() {
            return Ok(Command::none());
        }
    }
    let messages = WalletInterface::get_wallet_updater_status(grin_gui.wallet_interface.clone())?;
    let state = &mut grin_gui.wallet_state.operation_state.home_state;
    let last_message = messages.get(0);
    if let Some(m) = last_message {
        state.wallet_status = match m {
            StatusMessage::UpdatingOutputs(s) => format!("{}", s),
            StatusMessage::UpdatingTransactions(s) => format!("{}", s),
            StatusMessage::FullScanWarn(s) => format!("{}", s),
            StatusMessage::Scanning(s, m) => format!("Scanning - {}% complete", m),
            StatusMessage::ScanningComplete(s) => format!("{}", s),
            StatusMessage::UpdateWarning(s) => format!("{}", s),
        }
    }
    if time - state.last_summary_update
        > chrono::Duration::from_std(std::time::Duration::from_secs(10)).unwrap()
    {
        state.last_summary_update = chrono::Local::now();

        let w = grin_gui.wallet_interface.clone();

        let fut =
            move || WalletInterface::get_wallet_info(w.clone()).join(WalletInterface::get_txs(w));

        return Ok(Command::perform(fut(), |(wallet_info_res, txs_res)| {
            if wallet_info_res.is_err() {
                let e = wallet_info_res
                    .context("Failed to retrieve wallet info status")
                    .unwrap_err();
                return Message::Interaction(Interaction::WalletOperationHomeViewInteraction(
                    LocalViewInteraction::WalletInfoUpdateFailure(Arc::new(RwLock::new(Some(e)))),
                ));
            }
            if txs_res.is_err() {
                let e = txs_res
                    .context("Failed to retrieve wallet tx status")
                    .unwrap_err();
                return Message::Interaction(Interaction::WalletOperationHomeViewInteraction(
                    LocalViewInteraction::WalletInfoUpdateFailure(Arc::new(RwLock::new(Some(e)))),
                ));
            }
            let (node_success, wallet_info) = wallet_info_res.unwrap();
            let (_, txs) = txs_res.unwrap();
            Message::Interaction(Interaction::WalletOperationHomeViewInteraction(
                LocalViewInteraction::WalletInfoUpdateSuccess(node_success, wallet_info, txs),
            ))
        }));
    }
    // If slatepack address is not filled out, go get it
    let apply_tx_state = &mut grin_gui.wallet_state.operation_state.apply_tx_state;
    if apply_tx_state.address_value.is_empty() {
        let w = grin_gui.wallet_interface.clone();

        let fut = move || WalletInterface::get_slatepack_address(w.clone());
        return Ok(Command::perform(fut(), |get_slatepack_address_res| {
            if get_slatepack_address_res.is_err() {
                let e = get_slatepack_address_res
                    .context("Failed to retrieve wallet slatepack address")
                    .unwrap_err();
                return Message::Interaction(Interaction::WalletOperationHomeViewInteraction(
                    LocalViewInteraction::WalletInfoUpdateFailure(Arc::new(RwLock::new(Some(e)))),
                ));
            }
            Message::Interaction(Interaction::WalletOperationHomeViewInteraction(
                LocalViewInteraction::WalletSlatepackAddressUpdateSuccess(
                    get_slatepack_address_res.unwrap(),
                ),
            ))
        }));
    }
    Ok(Command::none())
}

pub fn handle_message<'a>(
    grin_gui: &mut GrinGui,
    message: LocalViewInteraction,
) -> Result<Command<Message>> {
    let state = &mut grin_gui.wallet_state.operation_state.home_state;
    match message {
        LocalViewInteraction::Back => {
            let wallet_interface = grin_gui.wallet_interface.clone();
            let fut = WalletInterface::close_wallet(wallet_interface);

            return Ok(Command::perform(fut, |r| {
                match r.context("Failed to close wallet") {
                    Ok(()) => {
                        Message::Interaction(Interaction::WalletOperationHomeViewInteraction(
                            LocalViewInteraction::WalletCloseSuccess,
                        ))
                    }
                    Err(e) => {
                        Message::Interaction(Interaction::WalletOperationHomeViewInteraction(
                            LocalViewInteraction::WalletCloseError(Arc::new(RwLock::new(Some(e)))),
                        ))
                    }
                }
            }));
        }
        LocalViewInteraction::Submit => {}
        LocalViewInteraction::WalletInfoUpdateSuccess(node_success, wallet_info, txs) => {
            debug!(
                "Update Wallet Info Summary: {}, {:?}",
                node_success, wallet_info
            );
            state.wallet_info = Some(wallet_info);
            debug!("Update Wallet Txs Summary: {:?}", txs);
            let tx_wrap_list = txs
                .iter()
                .map(|tx| TxLogEntryWrap::new(tx.clone()))
                .collect();
            state.wallet_txs = TxList { txs: tx_wrap_list };

            // experimental
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let now = chrono::Utc::now();
            state.chart.push_data(now, rng.gen_range(0..100));
        }
        LocalViewInteraction::WalletInfoUpdateFailure(err) => {
            grin_gui.error = err.write().unwrap().take();
            if let Some(e) = grin_gui.error.as_ref() {
                log_error(e);
            }
        }
        LocalViewInteraction::WalletCloseSuccess => {
            grin_gui.wallet_state.operation_state.mode =
                crate::gui::element::wallet::operation::Mode::Open;
        }
        LocalViewInteraction::WalletCloseError(err) => {
            grin_gui.error = err.write().unwrap().take();
            if let Some(e) = grin_gui.error.as_ref() {
                log_error(e);
            }
        }
        LocalViewInteraction::WalletSlatepackAddressUpdateSuccess(address) => {
            grin_gui
                .wallet_state
                .operation_state
                .apply_tx_state
                .address_value = address;
        }
        LocalViewInteraction::CancelTx(id) => {
            debug!("Cancel Tx: {}", id);
            grin_gui.error.take();

            log::debug!("Interaction::WalletOperationHomeViewInteraction::CancelTx");

            let w = grin_gui.wallet_interface.clone();

            let fut = move || WalletInterface::cancel_tx(w, id);

            return Ok(Command::perform(fut(), |r| {
                match r.context("Failed to Cancel Transaction") {
                    Ok(ret) => {
                        Message::Interaction(Interaction::WalletOperationHomeViewInteraction(
                            LocalViewInteraction::TxCancelledOk(ret),
                        ))
                    }
                    Err(e) => {
                        Message::Interaction(Interaction::WalletOperationHomeViewInteraction(
                            LocalViewInteraction::TxCancelError(Arc::new(RwLock::new(Some(e)))),
                        ))
                    }
                }
            }));
        }
        LocalViewInteraction::TxCancelledOk(id) => {
            //TODO: Message
            debug!("TX cancelled okay: {}", id);
        }
        LocalViewInteraction::TxCancelError(err) => {
            grin_gui.error = err.write().unwrap().take();
            if let Some(e) = grin_gui.error.as_ref() {
                log_error(e);
            }
        }
    }
    Ok(Command::none())
}

pub fn data_container<'a>(config: &'a Config, state: &'a StateContainer) -> Container<'a, Message> {
    // Buttons to perform operations go here, but empty container for now
    let operations_menu = action_menu::data_container(config, &state.action_menu_state);

    // Basic Info "Box"
    let waiting_string = "---------";
    let (
        total_string,
        amount_spendable_string,
        awaiting_confirmation_string,
        awaiting_finalization_string,
        locked_string,
    ) = match state.wallet_info.as_ref() {
        Some(info) => (
            amount_to_hr_string(info.total, false),
            amount_to_hr_string(info.amount_currently_spendable, false),
            amount_to_hr_string(info.amount_awaiting_confirmation, false),
            amount_to_hr_string(info.amount_awaiting_finalization, false),
            amount_to_hr_string(info.amount_locked, false),
        ),
        None => (
            waiting_string.to_owned(),
            waiting_string.to_owned(),
            waiting_string.to_owned(),
            waiting_string.to_owned(),
            waiting_string.to_owned(),
        ),
    };

    let wallet_name = config.wallets[config.current_wallet_index.unwrap()]
        .display_name
        .clone();

    // Title row
    let title = Text::new(amount_spendable_string.clone()).size(DEFAULT_HEADER_FONT_SIZE);
    let title_container =
        Container::new(title).style(grin_gui_core::theme::ContainerStyle::BrightBackground);

    let subtitle = Text::new(wallet_name).size(SMALLER_FONT_SIZE);
    let subtitle_container = Container::new(subtitle)
        .style(grin_gui_core::theme::ContainerStyle::NormalBackground)
        .padding(iced::Padding::from([
            3, // top
            0, // right
            0, // bottom
            0, // left
        ]));

    let close_wallet_label_container =
        Container::new(Text::new(localized_string("close")).size(SMALLER_FONT_SIZE))
            .height(Length::Units(14))
            .width(Length::Units(30))
            .center_y()
            .center_x();

    let close_wallet_button: Element<Interaction> = Button::new(close_wallet_label_container)
        .style(grin_gui_core::theme::ButtonStyle::Bordered)
        .on_press(Interaction::WalletOperationHomeViewInteraction(
            LocalViewInteraction::Back,
        ))
        .padding(2)
        .into();

    let subtitle_row = Row::new()
        .push(subtitle_container)
        .push(Space::with_width(Length::Units(2)))
        .push(close_wallet_button.map(Message::Interaction));

    let title_container = Container::new(Column::new().push(title_container).push(subtitle_row))
        .padding(iced::Padding::from([
            0, // top
            0, // right
            0, // bottom
            5, // left
        ]));

    let header_row = Row::new()
        .push(title_container)
        .push(Space::with_width(Length::Fill))
        .push(operations_menu);

    let header_container = Container::new(header_row).padding(iced::Padding::from([
        0,               // top
        0,               // right
        DEFAULT_PADDING, // bottom
        0,               // left
    ]));

    let total_value_label =
        Text::new(format!("{}:", localized_string("info-confirmed-total"))).size(DEFAULT_FONT_SIZE);
    let total_value_label_container = Container::new(total_value_label)
        .style(grin_gui_core::theme::ContainerStyle::BrightBackground);

    let total_value = Text::new(total_string).size(DEFAULT_FONT_SIZE);
    let total_value_container = Container::new(total_value)
        .style(grin_gui_core::theme::ContainerStyle::BrightBackground)
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Right);

    let total_row = Row::new()
        .push(total_value_label_container)
        .push(total_value_container)
        .width(Length::Fill);

    let awaiting_confirmation_label = Text::new(format!(
        "{}:",
        localized_string("info-awaiting-confirmation")
    ))
    .size(DEFAULT_FONT_SIZE);
    let awaiting_confirmation_label_container = Container::new(awaiting_confirmation_label)
        .style(grin_gui_core::theme::ContainerStyle::BrightBackground);

    let awaiting_confirmation_value =
        Text::new(awaiting_confirmation_string).size(DEFAULT_FONT_SIZE);
    let awaiting_confirmation_value_container = Container::new(awaiting_confirmation_value)
        .style(grin_gui_core::theme::ContainerStyle::BrightBackground)
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Right);

    let awaiting_confirmation_row = Row::new()
        .push(awaiting_confirmation_label_container)
        .push(awaiting_confirmation_value_container)
        .width(Length::Fill);

    let awaiting_finalization_label = Text::new(format!(
        "{}:",
        localized_string("info-awaiting-finalization")
    ))
    .size(DEFAULT_FONT_SIZE);
    let awaiting_finalization_label_container = Container::new(awaiting_finalization_label)
        .style(grin_gui_core::theme::ContainerStyle::BrightBackground);

    let awaiting_finalization_value =
        Text::new(awaiting_finalization_string).size(DEFAULT_FONT_SIZE);
    let awaiting_finalization_value_container = Container::new(awaiting_finalization_value)
        .style(grin_gui_core::theme::ContainerStyle::BrightBackground)
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Right);

    let awaiting_finalization_row = Row::new()
        .push(awaiting_finalization_label_container)
        .push(awaiting_finalization_value_container)
        .width(Length::Fill);

    let locked_label =
        Text::new(format!("{}:", localized_string("info-locked"))).size(DEFAULT_FONT_SIZE);
    let locked_label_container =
        Container::new(locked_label).style(grin_gui_core::theme::ContainerStyle::BrightBackground);

    let locked_value = Text::new(locked_string).size(DEFAULT_FONT_SIZE);
    let locked_value_container = Container::new(locked_value)
        .style(grin_gui_core::theme::ContainerStyle::BrightBackground)
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Right);

    let locked_row = Row::new()
        .push(locked_label_container)
        .push(locked_value_container)
        .width(Length::Fill);

    let amount_spendable_label =
        Text::new(format!("{}:", localized_string("info-amount-spendable")))
            .size(DEFAULT_FONT_SIZE);
    let amount_spendable_label_container = Container::new(amount_spendable_label)
        .style(grin_gui_core::theme::ContainerStyle::BrightBackground);

    let amount_spendable_value = Text::new(amount_spendable_string).size(DEFAULT_FONT_SIZE);
    let amount_spendable_value_container = Container::new(amount_spendable_value)
        .style(grin_gui_core::theme::ContainerStyle::BrightBackground)
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Right);

    let amount_spendable_row = Row::new()
        .push(amount_spendable_label_container)
        .push(amount_spendable_value_container)
        .width(Length::Fill);

    let info_column = Column::new()
        .push(total_row)
        .push(awaiting_confirmation_row)
        .push(awaiting_finalization_row)
        .push(locked_row)
        .push(amount_spendable_row)
        .spacing(7);

    let wallet_info_card_container = Container::new(info_column)
        .width(Length::Units(240))
        .padding(iced::Padding::from([
            DEFAULT_PADDING, // top
            DEFAULT_PADDING, // right
            DEFAULT_PADDING, // bottom
            5,               // left
        ]));

    let mut first_row_container = Row::new()
        .push(wallet_info_card_container)
        .push(state.chart.view(0))
        .height(Length::Units(120));

    // Status container bar at bottom of screen
    let status_container_label_text = Text::new(localized_string("status"))
        .size(DEFAULT_FONT_SIZE)
        .height(Length::Fill)
        .horizontal_alignment(alignment::Horizontal::Right)
        .vertical_alignment(alignment::Vertical::Center);

    let status_container_separator_text = Text::new(": ")
        .size(DEFAULT_FONT_SIZE)
        .height(Length::Fill)
        .horizontal_alignment(alignment::Horizontal::Right)
        .vertical_alignment(alignment::Vertical::Center);

    let status_container_status_text = Text::new(&state.wallet_status)
        .size(DEFAULT_FONT_SIZE)
        .height(Length::Fill)
        .horizontal_alignment(alignment::Horizontal::Right)
        .vertical_alignment(alignment::Vertical::Center);

    let status_container_contents = Row::new()
        .push(Space::new(Length::Fill, Length::Fill))
        .push(status_container_label_text)
        .push(status_container_separator_text)
        .push(status_container_status_text)
        .push(Space::new(Length::Units(DEFAULT_PADDING), Length::Units(0)));

    let status_container = Container::new(status_container_contents)
        .style(grin_gui_core::theme::ContainerStyle::BrightForeground)
        .height(Length::Fill)
        .width(Length::Fill)
        .style(grin_gui_core::theme::ContainerStyle::NormalBackground);

    let status_row = Row::new()
        .push(status_container)
        .height(Length::Units(25))
        .align_items(Alignment::Center)
        .spacing(25);

    // Temp Test Data
    use grin_gui_core::node::Identifier;
    /*let tx_list = TxList {
        txs: vec![
            TxLogEntry::new(Identifier::zero(), TxLogEntryType::ConfirmedCoinbase, 0),
            TxLogEntry::new(Identifier::zero(), TxLogEntryType::ConfirmedCoinbase, 1),
            TxLogEntry::new(Identifier::zero(), TxLogEntryType::ConfirmedCoinbase, 2),
            TxLogEntry::new(Identifier::zero(), TxLogEntryType::ConfirmedCoinbase, 3),
            TxLogEntry::new(Identifier::zero(), TxLogEntryType::ConfirmedCoinbase, 4),
            TxLogEntry::new(Identifier::zero(), TxLogEntryType::ConfirmedCoinbase, 5),
            TxLogEntry::new(Identifier::zero(), TxLogEntryType::ConfirmedCoinbase, 6),
        ],
    };*/

    let column_config = state.tx_header_state.column_config();

    // Tx row titles is a row of titles above the tx scrollable.
    // This is to add titles above each section of the tx row, to let
    // the user easily identify what the value is.
    let tx_row_titles = super::tx_list::titles_row_header(
        &state.wallet_txs,
        &state.tx_header_state.state,
        &state.tx_header_state.columns,
        state.tx_header_state.previous_column_key,
        state.tx_header_state.previous_sort_direction,
    );

    // A scrollable list containing rows.
    // Each row holds data about a single tx.
    let mut content = Column::new().spacing(1);
    //.height(Length::Fill)
    //.style(grin_gui_core::theme::ScrollableStyles::Primary);

    let mut has_txs = false;

    // Loops though the txs.
    for (idx, tx_wrap) in state.wallet_txs.txs.iter().enumerate() {
        has_txs = true;
        // If hiding ignored addons, we will skip it.
        /*if addon.state == AddonState::Ignored && self.config.hide_ignored_addons {
            continue;
        }*/

        // Skip addon if we are filter from query and addon doesn't have fuzzy score
        /*if query.is_some() && addon.fuzzy_score.is_none() {
            continue;
        }*/

        // Checks if the current tx is expanded.
        let is_tx_expanded = match &state.expanded_type {
            ExpandType::Details(a) => a.tx.id == tx_wrap.tx.id,
            ExpandType::None => false,
        };

        let is_odd = if config.alternating_row_colors {
            Some(idx % 2 != 0)
        } else {
            None
        };

        // A container cell which has all data about the current tx.
        // If the tx is expanded, then this is also included in this container.
        let tx_data_cell = tx_list::data_row_container(
            tx_wrap,
            is_tx_expanded,
            &state.expanded_type,
            config,
            &column_config,
            is_odd,
            &None,
        );

        // Adds the addon data cell to the scrollable.
        content = content.push(tx_data_cell);
    }

    let mut tx_list_scrollable =
        Scrollable::new(content).style(grin_gui_core::theme::ScrollableStyle::Primary);

    // Bottom space below the scrollable.
    let bottom_space = Space::new(Length::FillPortion(1), Length::Units(DEFAULT_PADDING));

    // This column gathers all the tx list elements together.
    let mut tx_list_content = Column::new();

    // Adds the rest of the elements to the content column.
    if has_txs {
        //TODO: Header widget is crashing specatularly on windows after iced-rs 0.5.0 update
        // disable header column until we figure out why and how to fix
        tx_list_content = tx_list_content.push(tx_row_titles).push(tx_list_scrollable);
        //tx_list_content = tx_list_content.push(tx_list_scrollable);
    }

    // Overall Home screen layout column
    let column = Column::new()
        .push(header_container)
        .push(first_row_container)
        .push(tx_list_content)
        .push(Space::new(Length::Units(0), Length::Fill))
        .push(status_row)
        .padding(10);
    //.align_items(Alignment::Center);

    Container::new(column).padding(iced::Padding::from([
        DEFAULT_PADDING, // top
        DEFAULT_PADDING, // right
        DEFAULT_PADDING, // bottom
        DEFAULT_PADDING, // left
    ]))
}
