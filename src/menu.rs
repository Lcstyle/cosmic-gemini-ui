use std::collections::HashMap;

use cosmic::widget;
use cosmic::widget::menu::action::MenuAction;

use crate::message::AppMessage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuActionItem {
    Home,
    NewTab,
    Back,
    Forward,
    Reload,
    Bookmark,
    IdentityManager,
    FocusUrlBar,
}

impl MenuAction for MenuActionItem {
    type Message = AppMessage;

    fn message(&self) -> Self::Message {
        match self {
            MenuActionItem::Home => AppMessage::GoHome,
            MenuActionItem::NewTab => AppMessage::NewTab,
            MenuActionItem::Back => AppMessage::Back,
            MenuActionItem::Forward => AppMessage::Forward,
            MenuActionItem::Reload => AppMessage::Reload,
            MenuActionItem::Bookmark => AppMessage::ToggleBookmark,
            MenuActionItem::IdentityManager => AppMessage::ShowIdentityManager,
            MenuActionItem::FocusUrlBar => AppMessage::FocusUrlBar,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabContextAction {
    NewTab,
    CloseTab,
    BookmarkTab,
}

impl MenuAction for TabContextAction {
    type Message = AppMessage;

    fn message(&self) -> Self::Message {
        match self {
            TabContextAction::NewTab => AppMessage::NewTab,
            TabContextAction::CloseTab => AppMessage::ContextCloseTab,
            TabContextAction::BookmarkTab => AppMessage::ContextBookmarkTab,
        }
    }
}

pub fn tab_context_menu_items() -> Vec<widget::menu::Tree<AppMessage>> {
    widget::menu::items(
        &HashMap::new(),
        vec![
            widget::menu::Item::Button("New Tab", None, TabContextAction::NewTab),
            widget::menu::Item::Divider,
            widget::menu::Item::Button("Bookmark", None, TabContextAction::BookmarkTab),
            widget::menu::Item::Divider,
            widget::menu::Item::Button("Close Tab", None, TabContextAction::CloseTab),
        ],
    )
}
