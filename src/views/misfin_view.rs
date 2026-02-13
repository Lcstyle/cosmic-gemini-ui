use cosmic::iced::Length;
use cosmic::widget;
use cosmic::Element;

use crate::message::AppMessage;

/// Misfin compose message view.
pub fn compose_view<'a>(
    recipient: &'a str,
    message: &'a str,
    identity_id: &'a Option<String>,
    identities: &'a [(String, String)], // (id, name)
    char_count: usize,
) -> Element<'a, AppMessage> {
    let max_chars: usize = 2048;

    let mut col = widget::column()
        .spacing(12)
        .padding(24)
        .width(Length::Fill)
        .max_width(700);

    col = col.push(
        widget::text::title3("Send Misfin Message")
            .width(Length::Fill),
    );

    // Recipient
    col = col.push(
        widget::text::body(format!("To: {}", recipient))
            .width(Length::Fill),
    );

    // Identity selector
    col = col.push(
        widget::text::body("From (identity):")
            .width(Length::Fill),
    );

    if identities.is_empty() {
        col = col.push(
            widget::text::body("No identities available. Create one first (Ctrl+I).")
                .width(Length::Fill),
        );
    } else {
        for (id, name) in identities {
            let is_selected = identity_id.as_deref() == Some(id.as_str());
            let label = if is_selected {
                format!("[*] {}", name)
            } else {
                name.clone()
            };
            let id_clone = id.clone();
            col = col.push(
                widget::button::standard(label)
                    .on_press(AppMessage::MisfinIdentitySelected(id_clone))
                    .width(Length::Fill),
            );
        }
    }

    col = col.push(widget::Space::with_height(4));

    // Message body
    col = col.push(
        widget::text::body("Message:")
            .width(Length::Fill),
    );
    col = col.push(
        widget::text_input("Type your message...", message)
            .on_input(AppMessage::MisfinMessageChanged)
            .width(Length::Fill),
    );

    // Character counter
    let counter_text = format!("{} / {}", char_count, max_chars);
    col = col.push(
        widget::text::caption(counter_text)
            .width(Length::Fill),
    );

    col = col.push(widget::Space::with_height(8));

    // Buttons
    let mut btn_row = widget::row().spacing(8);

    let can_send = identity_id.is_some() && !message.is_empty() && char_count <= max_chars;
    let mut send_btn = widget::button::suggested("Send");
    if can_send {
        send_btn = send_btn.on_press(AppMessage::MisfinSend);
    }
    btn_row = btn_row.push(send_btn);

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

/// Misfin sent result view.
pub fn sent_view<'a>(
    recipient: &'a str,
    status: &'a str,
) -> Element<'a, AppMessage> {
    let mut col = widget::column()
        .spacing(12)
        .padding(24)
        .width(Length::Fill)
        .max_width(600);

    col = col.push(
        widget::text::title3("Message Sent")
            .width(Length::Fill),
    );

    col = col.push(
        widget::text::body(format!("To: {}", recipient))
            .width(Length::Fill),
    );

    col = col.push(
        widget::text::body(format!("Status: {}", status))
            .width(Length::Fill),
    );

    col = col.push(widget::Space::with_height(8));

    col = col.push(
        widget::button::standard("Back")
            .on_press(AppMessage::Back),
    );

    widget::container(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .into()
}
