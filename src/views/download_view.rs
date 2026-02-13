use cosmic::iced::Length;
use cosmic::widget;
use cosmic::Element;

use crate::message::AppMessage;

/// Render the download progress view.
pub fn view<'a>(
    filename: &'a str,
    bytes_downloaded: u64,
    complete: bool,
    path: Option<&'a str>,
) -> Element<'a, AppMessage> {
    let size_text = if bytes_downloaded < 1024 {
        format!("{} B", bytes_downloaded)
    } else if bytes_downloaded < 1024 * 1024 {
        format!("{:.1} KB", bytes_downloaded as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes_downloaded as f64 / (1024.0 * 1024.0))
    };

    let mut col = widget::column()
        .push(widget::text::title3("Download"))
        .push(widget::Space::with_height(16))
        .push(widget::text::body(filename))
        .push(widget::Space::with_height(8))
        .push(widget::text::body(size_text));

    if complete {
        col = col
            .push(widget::Space::with_height(8))
            .push(widget::text::body("Download complete."));

        if let Some(p) = path {
            col = col.push(widget::Space::with_height(8)).push(
                widget::button::standard("Open File")
                    .on_press(AppMessage::OpenDownload(p.to_string())),
            );
        }
    } else {
        col = col
            .push(widget::Space::with_height(8))
            .push(widget::text::body("Downloading..."));
    }

    widget::container(col.spacing(4).padding(24).width(Length::Fill).max_width(600))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .into()
}
