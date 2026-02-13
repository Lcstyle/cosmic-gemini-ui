use cosmic::iced::Length;
use cosmic::widget;
use cosmic::Element;

use crate::message::AppMessage;

/// Render an error page.
pub fn view(message: &str) -> Element<'_, AppMessage> {
    widget::container(
        widget::column()
            .push(
                widget::text::title3("Error")
                    .width(Length::Fill),
            )
            .push(widget::Space::with_height(16))
            .push(
                widget::text::body(message)
                    .width(Length::Fill),
            )
            .spacing(8)
            .padding(24)
            .width(Length::Fill)
            .max_width(600),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .into()
}
