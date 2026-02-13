use cosmic::iced::Length;
use cosmic::widget;
use cosmic::Element;

use crate::message::AppMessage;

/// Titan upload form view.
pub fn view<'a>(
    url: &'a str,
    mime: &'a str,
    text: &'a str,
    token: &'a str,
) -> Element<'a, AppMessage> {
    let mut col = widget::column()
        .spacing(12)
        .padding(24)
        .width(Length::Fill)
        .max_width(700);

    col = col.push(
        widget::text::title3("Titan Upload")
            .width(Length::Fill),
    );

    col = col.push(
        widget::text::body(format!("Upload to: {}", url))
            .width(Length::Fill),
    );

    col = col.push(widget::Space::with_height(4));

    // MIME type
    col = col.push(
        widget::text::body("MIME Type:")
            .width(Length::Fill),
    );
    col = col.push(
        widget::text_input("text/gemini", mime)
            .on_input(AppMessage::TitanMimeChanged)
            .width(Length::Fill),
    );

    // Token (optional)
    col = col.push(
        widget::text::body("Token (optional):")
            .width(Length::Fill),
    );
    col = col.push(
        widget::text_input("Authentication token", token)
            .on_input(AppMessage::TitanTokenChanged)
            .width(Length::Fill),
    );

    // Text content
    col = col.push(
        widget::text::body("Content:")
            .width(Length::Fill),
    );
    col = col.push(
        widget::text_input("Enter content to upload...", text)
            .on_input(AppMessage::TitanTextChanged)
            .width(Length::Fill),
    );

    col = col.push(widget::Space::with_height(8));

    // Buttons
    let mut btn_row = widget::row().spacing(8);

    let can_submit = !text.is_empty();
    let mut submit_btn = widget::button::suggested("Upload");
    if can_submit {
        submit_btn = submit_btn.on_press(AppMessage::TitanSubmit);
    }
    btn_row = btn_row.push(submit_btn);

    btn_row = btn_row.push(
        widget::button::standard("Cancel")
            .on_press(AppMessage::Back),
    );

    col = col.push(btn_row);

    widget::scrollable(
        widget::container(col)
            .width(Length::Fill)
            .center_x(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
