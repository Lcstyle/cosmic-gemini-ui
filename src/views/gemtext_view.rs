use cosmic::iced::{ContentFit, Length};
use cosmic::widget;
use cosmic::Element;

use crate::message::AppMessage;
use crate::tab::Block;

/// Render a list of gemtext blocks into an iced widget column.
pub fn view<'a>(blocks: &'a [Block]) -> Element<'a, AppMessage> {
    let mut col = widget::column().spacing(4).width(Length::Fill);

    for block in blocks {
        col = col.push(render_block(block));
    }

    widget::scrollable(
        widget::container(col)
            .padding(16)
            .width(Length::Fill)
            .max_width(800),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn render_block<'a>(block: &'a Block) -> Element<'a, AppMessage> {
    match block {
        Block::Heading { level, text } => {
            let size = match level {
                1 => 28.0,
                2 => 22.0,
                _ => 18.0,
            };
            widget::text::title3(text.as_str())
                .size(size)
                .width(Length::Fill)
                .into()
        }
        Block::Paragraph(text) => {
            widget::text::body(text.as_str())
                .width(Length::Fill)
                .into()
        }
        Block::Link { href, label } => {
            let display = if label.is_empty() { href.clone() } else { label.clone() };
            widget::button::link(display)
                .on_press(AppMessage::LinkClicked(href.clone()))
                .into()
        }
        Block::PreBlock { lines } => {
            let text = lines.join("\n");
            widget::container(
                widget::text::body(text)
                    .font(cosmic::iced::Font::MONOSPACE)
                    .width(Length::Fill),
            )
            .padding(8)
            .width(Length::Fill)
            .class(cosmic::theme::Container::Card)
            .into()
        }
        Block::Quote(text) => {
            widget::container(
                widget::text::body(text.as_str())
                    .width(Length::Fill),
            )
            .padding([4, 8, 4, 16])
            .width(Length::Fill)
            .class(cosmic::theme::Container::Card)
            .into()
        }
        Block::ListItem(text) => {
            widget::row()
                .push(widget::text::body(" \u{2022} "))
                .push(
                    widget::text::body(text.as_str())
                        .width(Length::Fill),
                )
                .spacing(4)
                .into()
        }
        Block::Image { alt, url, data } => {
            match data {
                Some(bytes) => {
                    let handle = widget::image::Handle::from_bytes(bytes.clone());
                    widget::column()
                        .push(
                            widget::image(handle)
                                .width(Length::Fill)
                                .content_fit(ContentFit::ScaleDown),
                        )
                        .push(widget::text::caption(alt.as_str()))
                        .spacing(4)
                        .into()
                }
                None => {
                    let display = if alt.is_empty() {
                        format!("[image: {}]", url)
                    } else {
                        format!("[image: {}]", alt)
                    };
                    widget::button::link(display)
                        .on_press(AppMessage::LinkClicked(url.clone()))
                        .into()
                }
            }
        }
        Block::Blank => {
            widget::Space::with_height(8).into()
        }
    }
}
