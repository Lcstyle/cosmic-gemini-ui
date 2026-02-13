use cosmic::iced::Length;
use cosmic::widget;
use cosmic::Element;

use crate::message::AppMessage;

/// Identity selector view — shown when a server returns status 6x.
pub fn selector_view<'a>(
    url: &'a str,
    identities: &'a [(String, String, String)],
) -> Element<'a, AppMessage> {
    let mut col = widget::column()
        .spacing(12)
        .padding(24)
        .width(Length::Fill)
        .max_width(600);

    col = col.push(
        widget::text::title3("Client Certificate Required")
            .width(Length::Fill),
    );

    col = col.push(
        widget::text::body(format!("The server at {} requires a client certificate to proceed.", url))
            .width(Length::Fill),
    );

    col = col.push(widget::Space::with_height(8));

    if identities.is_empty() {
        col = col.push(
            widget::text::body("No identities available. Create one to continue.")
                .width(Length::Fill),
        );
    } else {
        col = col.push(
            widget::text::body("Select an identity:")
                .width(Length::Fill),
        );

        for (id, name, fingerprint) in identities {
            let short_fp = if fingerprint.len() > 16 {
                format!("{}...", &fingerprint[..16])
            } else {
                fingerprint.clone()
            };

            let label = format!("{} ({})", name, short_fp);
            let url_clone = url.to_string();
            let id_clone = id.clone();

            col = col.push(
                widget::button::standard(label)
                    .on_press(AppMessage::SelectIdentity {
                        url: url_clone,
                        identity_id: id_clone,
                    })
                    .width(Length::Fill),
            );
        }
    }

    col = col.push(widget::Space::with_height(8));

    let mut btn_row = widget::row().spacing(8);

    btn_row = btn_row.push(
        widget::button::suggested("Create New Identity")
            .on_press(AppMessage::ShowIdentityManager),
    );

    btn_row = btn_row.push(
        widget::button::standard("Cancel")
            .on_press(AppMessage::Back),
    );

    col = col.push(btn_row);

    widget::container(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .into()
}

/// Identity manager view — list all identities, generate, import, delete.
/// identities: Vec of (id, name, fingerprint, created_at, expires_at)
/// bindings: Vec of (identity_id, hostnames)
pub fn manager_view<'a>(
    identities: &'a [(String, String, String, String, String)],
    bindings: &'a [(String, Vec<String>)],
    new_identity_name: &'a str,
) -> Element<'a, AppMessage> {
    let mut col = widget::column()
        .spacing(12)
        .padding(24)
        .width(Length::Fill)
        .max_width(700);

    col = col.push(
        widget::text::title3("Identity Manager")
            .width(Length::Fill),
    );

    col = col.push(widget::Space::with_height(4));

    if identities.is_empty() {
        col = col.push(
            widget::text::body("No identities yet. Enter a name and generate one below.")
                .width(Length::Fill),
        );
    } else {
        for (id, name, fingerprint, created_at, expires_at) in identities {
            let short_fp = if fingerprint.len() > 24 {
                format!("{}...", &fingerprint[..24])
            } else {
                fingerprint.clone()
            };

            // Find bound hosts for this identity
            let bound_hosts: Vec<String> = bindings
                .iter()
                .filter(|(bind_id, _)| bind_id == id)
                .flat_map(|(_, hosts)| hosts.clone())
                .collect();

            let mut card_col = widget::column().spacing(4);

            card_col = card_col.push(
                widget::text::body(format!("Name: {}", name))
                    .width(Length::Fill),
            );
            card_col = card_col.push(
                widget::text::caption(format!("Fingerprint: {}", short_fp))
                    .width(Length::Fill),
            );
            card_col = card_col.push(
                widget::text::caption(format!("Created: {}", format_date(created_at)))
                    .width(Length::Fill),
            );
            if !expires_at.is_empty() {
                card_col = card_col.push(
                    widget::text::caption(format!("Expires: {}", format_date(expires_at)))
                        .width(Length::Fill),
                );
            }

            if !bound_hosts.is_empty() {
                card_col = card_col.push(
                    widget::text::caption(format!("Bound to: {}", bound_hosts.join(", ")))
                        .width(Length::Fill),
                );

                for host in &bound_hosts {
                    let host_clone = host.clone();
                    card_col = card_col.push(
                        widget::button::destructive(format!("Unbind {}", host))
                            .on_press(AppMessage::UnbindHost(host_clone)),
                    );
                }
            }

            let id_clone = id.clone();
            card_col = card_col.push(
                widget::button::destructive("Delete")
                    .on_press(AppMessage::DeleteIdentity(id_clone)),
            );

            col = col.push(
                widget::container(card_col)
                    .padding(12)
                    .width(Length::Fill)
                    .class(cosmic::theme::Container::Card),
            );
        }
    }

    col = col.push(widget::Space::with_height(8));

    // New identity creation
    col = col.push(
        widget::text::body("Generate New Identity:")
            .width(Length::Fill),
    );

    col = col.push(
        widget::text_input("Identity name (e.g. your username)", new_identity_name)
            .on_input(AppMessage::IdentityNameChanged)
            .width(Length::Fill),
    );

    let name_valid = !new_identity_name.trim().is_empty();

    let mut gen_row = widget::row().spacing(8);
    for (label, days) in [("1 Year", 365u64), ("5 Years", 1825), ("10 Years", 3650)] {
        let name = if name_valid {
            new_identity_name.trim().to_string()
        } else {
            format!("Identity ({})", label)
        };
        let mut btn = widget::button::suggested(label);
        if name_valid {
            btn = btn.on_press(AppMessage::CreateIdentity {
                name,
                duration_days: days,
            });
        }
        gen_row = gen_row.push(btn);
    }
    col = col.push(gen_row);

    if !name_valid {
        col = col.push(
            widget::text::caption("Enter a name above to enable identity generation")
                .width(Length::Fill),
        );
    }

    widget::scrollable(
        widget::container(col)
            .width(Length::Fill)
            .center_x(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn format_date(iso: &str) -> String {
    // Show just the date portion of ISO 8601
    if let Some(date) = iso.split('T').next() {
        date.to_string()
    } else {
        iso.to_string()
    }
}
