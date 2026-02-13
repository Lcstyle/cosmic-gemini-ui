use cosmic::iced::Length;
use cosmic::widget;
use cosmic::Element;

use crate::message::AppMessage;

/// Render a certificate warning page.
pub fn view<'a>(url: &'a str, error: &'a str) -> Element<'a, AppMessage> {
    widget::container(
        widget::column()
            .push(
                widget::text::title3("Certificate Warning")
                    .width(Length::Fill),
            )
            .push(widget::Space::with_height(16))
            .push(
                widget::text::body(
                    "The certificate for this server has changed since your last visit. \
                     This could indicate a security issue (man-in-the-middle attack) \
                     or the server may have simply renewed its certificate.",
                )
                .width(Length::Fill),
            )
            .push(widget::Space::with_height(8))
            .push(
                widget::text::body(format!("Host: {}", url))
                    .width(Length::Fill),
            )
            .push(
                widget::text::body(format!("Error: {}", error))
                    .width(Length::Fill),
            )
            .push(widget::Space::with_height(16))
            .push(
                widget::row()
                    .push(
                        widget::button::suggested("Trust New Certificate")
                            .on_press(AppMessage::TrustCertificate(url.to_string())),
                    )
                    .push(widget::Space::with_width(8))
                    .push(
                        widget::button::standard("Go Back")
                            .on_press(AppMessage::Back),
                    )
                    .spacing(8),
            )
            .spacing(4)
            .padding(24)
            .width(Length::Fill)
            .max_width(600),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .into()
}
