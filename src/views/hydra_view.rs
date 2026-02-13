use cosmic::iced::Length;
use cosmic::widget;
use cosmic::Element;

use crate::message::AppMessage;

/// Render the HYDRA panel view.
pub fn view<'a>(
    status: Option<&'a hydra_core::node::HydraStatus>,
    alerts: &'a [hydra_core::alert::AlertResult],
    new_peer_address: &'a str,
) -> Element<'a, AppMessage> {
    let mut col = widget::column().spacing(16).padding(24).width(Length::Fill);

    // Title
    col = col.push(widget::text::title3("HYDRA Protocol").width(Length::Fill));

    match status {
        Some(status) => {
            col = col.push(node_info_section(status));
            col = col.push(widget::divider::horizontal::default());
            col = col.push(stats_section(status));
            col = col.push(widget::divider::horizontal::default());
            col = col.push(controls_section(status));

            if !alerts.is_empty() {
                col = col.push(widget::divider::horizontal::default());
                col = col.push(alerts_section(alerts));
            }

            col = col.push(widget::divider::horizontal::default());
            col = col.push(peers_section(status, new_peer_address));
        }
        None => {
            col = col.push(
                widget::text::body("HYDRA is not running. Enable it to start observing certificates.")
                    .width(Length::Fill),
            );
            col = col.push(
                widget::button::suggested("Enable HYDRA")
                    .on_press(AppMessage::HydraToggleEnabled),
            );
        }
    }

    widget::container(
        widget::scrollable(col.max_width(700))
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .into()
}

/// Node identity and stage info.
fn node_info_section<'a>(status: &'a hydra_core::node::HydraStatus) -> Element<'a, AppMessage> {
    let stage_label = match status.stage {
        0 => "Stage 0 (Seed) - Local observation only",
        1 => "Stage 1 (Pair) - Bilateral sync active",
        _ => "Unknown stage",
    };

    let node_id_short = if status.node_id.len() > 16 {
        format!("{}...", &status.node_id[..16])
    } else {
        status.node_id.clone()
    };

    let net_id_short = if status.network_id.len() > 16 {
        format!("{}...", &status.network_id[..16])
    } else {
        status.network_id.clone()
    };

    widget::column()
        .push(widget::text::title4("Node Identity").width(Length::Fill))
        .push(widget::Space::with_height(4))
        .push(widget::text::body(format!("Node ID: {}", node_id_short)).width(Length::Fill))
        .push(widget::text::body(format!("Network: {}", net_id_short)).width(Length::Fill))
        .push(widget::text::body(stage_label).width(Length::Fill))
        .push(widget::text::body(format!(
            "Status: {}",
            if status.enabled { "Active" } else { "Disabled" }
        )).width(Length::Fill))
        .spacing(2)
        .into()
}

/// Statistics section.
fn stats_section<'a>(status: &'a hydra_core::node::HydraStatus) -> Element<'a, AppMessage> {
    widget::column()
        .push(widget::text::title4("Statistics").width(Length::Fill))
        .push(widget::Space::with_height(4))
        .push(
            widget::row()
                .push(stat_item("Observations", status.observation_count))
                .push(stat_item("Events", status.event_count))
                .push(stat_item("Domains", status.domain_count))
                .push(stat_item("Peers", status.peer_count))
                .push(stat_item("Alerts", status.alert_count))
                .spacing(24),
        )
        .spacing(2)
        .into()
}

fn stat_item<'a>(label: &'a str, value: usize) -> Element<'a, AppMessage> {
    widget::column()
        .push(widget::text::title4(format!("{}", value)).width(Length::Fill))
        .push(widget::text::caption(label).width(Length::Fill))
        .spacing(2)
        .width(Length::Fill)
        .into()
}

/// Controls section: toggle, sync.
fn controls_section<'a>(status: &'a hydra_core::node::HydraStatus) -> Element<'a, AppMessage> {
    let toggle_label = if status.enabled {
        "Disable HYDRA"
    } else {
        "Enable HYDRA"
    };

    let mut row = widget::row().spacing(8);
    row = row.push(
        widget::button::standard(toggle_label).on_press(AppMessage::HydraToggleEnabled),
    );

    if status.enabled && status.peer_count > 0 {
        row = row.push(
            widget::button::suggested("Manual Sync").on_press(AppMessage::HydraManualSync),
        );
    }

    widget::column()
        .push(widget::text::title4("Controls").width(Length::Fill))
        .push(widget::Space::with_height(4))
        .push(row)
        .spacing(2)
        .into()
}

/// Alerts section.
fn alerts_section<'a>(alerts: &'a [hydra_core::alert::AlertResult]) -> Element<'a, AppMessage> {
    let mut col = widget::column()
        .push(widget::text::title4("Alerts").width(Length::Fill))
        .push(widget::Space::with_height(4))
        .spacing(4);

    for (i, alert) in alerts.iter().enumerate() {
        let level_str = format!("{:?}", alert.level);
        let msg = if alert.message.is_empty() { "No details" } else { &alert.message };

        let alert_row = widget::row()
            .push(
                widget::column()
                    .push(
                        widget::text::body(format!(
                            "[{}] {} - {}",
                            level_str, alert.domain, msg
                        ))
                        .width(Length::Fill),
                    )
                    .width(Length::Fill),
            )
            .push(
                widget::button::standard("Dismiss")
                    .on_press(AppMessage::HydraDismissAlert(i)),
            )
            .spacing(8)
            .align_y(cosmic::iced::Alignment::Center);

        col = col.push(alert_row);
    }

    col.into()
}

/// Peers section: list, add, remove.
fn peers_section<'a>(
    status: &'a hydra_core::node::HydraStatus,
    new_peer_address: &'a str,
) -> Element<'a, AppMessage> {
    let mut col = widget::column()
        .push(widget::text::title4("Peers").width(Length::Fill))
        .push(widget::Space::with_height(4))
        .spacing(4);

    if status.peer_count == 0 {
        col = col.push(
            widget::text::body("No peers connected. Add a peer's .onion address to enable Stage 1 bilateral sync.")
                .width(Length::Fill),
        );
    } else {
        col = col.push(
            widget::text::body(format!("{} peer(s) connected", status.peer_count))
                .width(Length::Fill),
        );
    }

    // Add peer input
    let add_row = widget::row()
        .push(
            widget::text_input("Peer .onion address...", new_peer_address)
                .on_input(AppMessage::HydraPeerAddressChanged)
                .width(Length::Fill),
        )
        .push(
            widget::button::suggested("Add Peer")
                .on_press_maybe(if new_peer_address.is_empty() {
                    None
                } else {
                    Some(AppMessage::HydraAddPeer(new_peer_address.to_string()))
                }),
        )
        .spacing(8)
        .align_y(cosmic::iced::Alignment::Center);

    col = col.push(widget::Space::with_height(8));
    col = col.push(add_row);

    col.into()
}
