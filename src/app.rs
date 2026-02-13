use std::time::Duration;

use cosmic::app::Core;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::keyboard::{self, key::Named, Key, Modifiers};
use cosmic::iced::{Length, Subscription};
use cosmic::widget::segmented_button;
use cosmic::{widget, Action, Element, Task};

use crate::config::{AppConfig, APP_ID};
use crate::menu::tab_context_menu_items;
use crate::message::AppMessage;
use crate::model::AppModel;
use crate::tab::TabContent;
use crate::update;
use crate::views;

/// Flags passed into the application on startup.
#[derive(Debug, Clone)]
pub struct Flags {
    pub url: Option<String>,
}

impl Default for Flags {
    fn default() -> Self {
        Self { url: None }
    }
}

/// Main COSMIC application struct.
pub struct App {
    core: Core,
    pub model: AppModel,
    pub config: AppConfig,
    config_handler: Option<cosmic_config::Config>,
    pub tab_model: segmented_button::SingleSelectModel,
    pub context_tab: Option<segmented_button::Entity>,
}

impl App {
    /// Rebuild the `tab_model` from the current `model.tabs` state.
    pub fn rebuild_tab_model(&mut self) {
        self.tab_model = segmented_button::SingleSelectModel::default();
        for (i, tab) in self.model.tabs.iter().enumerate() {
            let mut inserter = self.tab_model.insert().text(tab.title.clone()).closable();
            if i == self.model.active_tab {
                inserter = inserter.activate();
            }
            let _ = inserter.id();
        }
    }
}

impl cosmic::Application for App {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = Flags;
    type Message = AppMessage;

    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        let (config, config_handler) =
            match cosmic_config::Config::new(Self::APP_ID, AppConfig::VERSION) {
                Ok(handler) => {
                    let config = AppConfig::get_entry(&handler).unwrap_or_default();
                    (config, Some(handler))
                }
                Err(_) => (AppConfig::default(), None),
            };

        let model = AppModel::new();

        let mut app = Self {
            core,
            model,
            config,
            config_handler,
            tab_model: segmented_button::SingleSelectModel::default(),
            context_tab: None,
        };

        // If a URL was passed, navigate to it; otherwise try to restore session
        let init_task = if let Some(url) = flags.url {
            app.model.url_bar_text = url.clone();
            update::update(&mut app, &AppMessage::Navigate(url))
        } else if let Ok(Some(session)) = gemini_core::session::load_session() {
            app.model.restore_from_session(session);
            // Re-fetch active tab
            let url = app.model.active_tab().url.clone();
            if !url.is_empty() {
                app.model.active_tab_mut().content = crate::tab::TabContent::Loading;
                update::update(&mut app, &AppMessage::Navigate(url))
            } else {
                update::load_bookmarks_into_active_tab(&mut app);
                Task::none()
            }
        } else if !app.config.home_page.is_empty() {
            let url = app.config.home_page.clone();
            app.model.url_bar_text = url.clone();
            app.model.active_tab_mut().content = crate::tab::TabContent::Loading;
            update::update(&mut app, &AppMessage::Navigate(url))
        } else {
            update::load_bookmarks_into_active_tab(&mut app);
            Task::none()
        };

        app.rebuild_tab_model();
        (app, init_task)
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        update::update(self, &message)
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        views::header::header_start(&self.model)
    }

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        views::header::header_end(&self.model)
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let tab = self.model.active_tab();

        let content: Element<'_, Self::Message> = match &tab.content {
            TabContent::Loading => {
                cosmic::widget::container(
                    cosmic::widget::text::body("Loading...")
                        .width(Length::Fill),
                )
                .padding(24)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .into()
            }
            TabContent::Document(blocks) => {
                views::gemtext_view::view(blocks)
            }
            TabContent::Error(msg) => {
                views::error_view::view(msg)
            }
            TabContent::Input { prompt, sensitive, value } => {
                let input_widget = if *sensitive {
                    cosmic::widget::secure_input("", value, None::<AppMessage>, false)
                        .on_input(AppMessage::InputChanged)
                        .on_submit(AppMessage::InputSubmitted)
                } else {
                    cosmic::widget::text_input(prompt.as_str(), value)
                        .on_input(AppMessage::InputChanged)
                        .on_submit(AppMessage::InputSubmitted)
                };

                cosmic::widget::container(
                    cosmic::widget::column()
                        .push(
                            cosmic::widget::text::title3(prompt.as_str())
                                .width(Length::Fill),
                        )
                        .push(cosmic::widget::Space::with_height(16))
                        .push(input_widget)
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
            TabContent::CertWarning { url, error } => {
                views::cert_warning_view::view(url, error)
            }
            TabContent::Download { filename, bytes_downloaded, complete, path } => {
                views::download_view::view(
                    filename,
                    *bytes_downloaded,
                    *complete,
                    path.as_deref(),
                )
            }
            TabContent::IdentityRequired { url, identities } => {
                views::identity_view::selector_view(url, identities)
            }
            TabContent::IdentityManager { identities, bindings, new_identity_name } => {
                views::identity_view::manager_view(identities, bindings, new_identity_name)
            }
            TabContent::TitanUpload { url, mime, text, token } => {
                views::titan_view::view(url, mime, text, token)
            }
            TabContent::MisfinCompose { recipient, message, identity_id, char_count, identities } => {
                views::misfin_view::compose_view(recipient, message, identity_id, identities, *char_count)
            }
            TabContent::MisfinSent { recipient, status } => {
                views::misfin_view::sent_view(recipient, status)
            }
            TabContent::Blank => {
                cosmic::widget::container(
                    cosmic::widget::column()
                        .push(
                            cosmic::widget::text::title3("Welcome to Cosmic Gemini")
                                .width(Length::Fill),
                        )
                        .push(cosmic::widget::Space::with_height(8))
                        .push(
                            cosmic::widget::text::body("Enter a Gemini URL above to get started.")
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
        };

        let tab_bar = widget::tab_bar::horizontal(&self.tab_model)
            .on_activate(AppMessage::TabActivate)
            .on_close(AppMessage::TabClose)
            .on_context(AppMessage::TabContext)
            .context_menu(Some(tab_context_menu_items()))
            .on_middle_press(AppMessage::TabClose)
            .width(Length::Fill);

        let new_tab_btn = cosmic::widget::button::icon(
            widget::icon::from_name("list-add-symbolic"),
        )
        .on_press(AppMessage::NewTab);

        let tab_row: Element<'_, Self::Message> = cosmic::widget::row()
            .push(tab_bar)
            .push(new_tab_btn)
            .align_y(cosmic::iced::Alignment::Center)
            .width(Length::Fill)
            .into();

        cosmic::widget::column()
            .push(tab_row)
            .push(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch([
            keyboard::on_key_press(handle_key_press),
            cosmic::iced::time::every(Duration::from_secs(30))
                .map(|_| AppMessage::SaveSession),
        ])
    }

    fn on_close_requested(&self, _id: cosmic::iced::window::Id) -> Option<AppMessage> {
        // Save session before closing
        let data = self.model.to_session_data();
        if let Err(e) = gemini_core::session::save_session(&data) {
            log::error!("Failed to save session on close: {}", e);
        }
        None // Allow close
    }
}

/// Map key presses to application messages.
fn handle_key_press(key: Key, modifiers: Modifiers) -> Option<AppMessage> {
    if modifiers.control() && !modifiers.shift() && !modifiers.alt() {
        return match key.as_ref() {
            Key::Character("l") => Some(AppMessage::FocusUrlBar),
            Key::Character("t") => Some(AppMessage::NewTab),
            Key::Character("w") => Some(AppMessage::CloseTab(usize::MAX)), // sentinel: close active
            Key::Character("r") => Some(AppMessage::Reload),
            Key::Character("d") => Some(AppMessage::ToggleBookmark),
            Key::Character("i") => Some(AppMessage::ShowIdentityManager),
            _ => None,
        };
    }

    if modifiers.alt() && !modifiers.control() {
        return match key.as_ref() {
            Key::Named(Named::ArrowLeft) => Some(AppMessage::Back),
            Key::Named(Named::ArrowRight) => Some(AppMessage::Forward),
            Key::Named(Named::Home) => Some(AppMessage::GoHome),
            _ => None,
        };
    }

    if !modifiers.control() && !modifiers.alt() && !modifiers.shift() {
        return match key.as_ref() {
            Key::Named(Named::F5) => Some(AppMessage::Reload),
            Key::Named(Named::F6) => Some(AppMessage::FocusUrlBar),
            _ => None,
        };
    }

    // Ctrl+Tab / Ctrl+Shift+Tab for tab switching
    if modifiers.control() {
        return match key.as_ref() {
            Key::Named(Named::Tab) => {
                if modifiers.shift() {
                    Some(AppMessage::PrevTab)
                } else {
                    Some(AppMessage::NextTab)
                }
            }
            _ => None,
        };
    }

    None
}
