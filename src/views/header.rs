use std::collections::HashMap;

use cosmic::iced::Length;
use cosmic::widget;
use cosmic::Element;

use crate::menu::MenuActionItem;
use crate::message::AppMessage;
use crate::model::AppModel;

/// Build the header bar elements (left side: nav buttons, URL bar).
pub fn header_start(model: &AppModel) -> Vec<Element<'_, AppMessage>> {
    let tab = model.active_tab();

    let back_btn = widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
        .on_press_maybe(if tab.can_go_back() {
            Some(AppMessage::Back)
        } else {
            None
        });

    let forward_btn = widget::button::icon(widget::icon::from_name("go-next-symbolic"))
        .on_press_maybe(if tab.can_go_forward() {
            Some(AppMessage::Forward)
        } else {
            None
        });

    let reload_btn = widget::button::icon(widget::icon::from_name("view-refresh-symbolic"))
        .on_press(AppMessage::Reload);

    let url_input = widget::text_input("Enter Gemini URL...", &model.url_bar_text)
        .on_input(AppMessage::UrlBarChanged)
        .on_submit(AppMessage::Navigate)
        .width(Length::Fill);

    vec![
        back_btn.into(),
        forward_btn.into(),
        reload_btn.into(),
        url_input.into(),
    ]
}

/// Build header bar elements (right side: HYDRA status, bookmarks, new tab, menu).
pub fn header_end(model: &AppModel) -> Vec<Element<'_, AppMessage>> {
    // HYDRA status icon: grey=off, green=active, amber=alerts
    let hydra_icon_name = match &model.hydra_status {
        Some(status) if status.enabled && model.hydra_alerts.is_empty() => {
            "security-high-symbolic"
        }
        Some(status) if status.enabled => "security-medium-symbolic",
        _ => "security-low-symbolic",
    };
    let hydra_btn = widget::button::icon(widget::icon::from_name(hydra_icon_name))
        .on_press(AppMessage::ShowHydraPanel);

    let bookmark_btn =
        widget::button::icon(widget::icon::from_name("bookmark-new-symbolic"))
            .on_press(AppMessage::ToggleBookmark);

    let new_tab_btn =
        widget::button::icon(widget::icon::from_name("tab-new-symbolic"))
            .on_press(AppMessage::NewTab);

    let menu_items = vec![
        widget::menu::Item::Button("Home", None, MenuActionItem::Home),
        widget::menu::Item::Button("New Tab", None, MenuActionItem::NewTab),
        widget::menu::Item::Divider,
        widget::menu::Item::Button("Back", None, MenuActionItem::Back),
        widget::menu::Item::Button("Forward", None, MenuActionItem::Forward),
        widget::menu::Item::Button("Reload", None, MenuActionItem::Reload),
        widget::menu::Item::Divider,
        widget::menu::Item::Button("Bookmark Page", None, MenuActionItem::Bookmark),
        widget::menu::Item::Button("Identity Manager", None, MenuActionItem::IdentityManager),
        widget::menu::Item::Button("HYDRA Panel", None, MenuActionItem::HydraPanel),
        widget::menu::Item::Divider,
        widget::menu::Item::Button("Focus URL Bar", None, MenuActionItem::FocusUrlBar),
    ];

    let menu_icon: Element<'_, AppMessage> =
        widget::button::icon(widget::icon::from_name("open-menu-symbolic")).into();
    let menu_tree = widget::menu::Tree::with_children(
        menu_icon,
        widget::menu::items(&HashMap::new(), menu_items),
    );

    let menu_bar = widget::menu::bar(vec![menu_tree]);

    vec![hydra_btn.into(), bookmark_btn.into(), new_tab_btn.into(), menu_bar.into()]
}
