use gemini_core::session::{SessionData, SessionHistoryEntry, SessionTab};

use crate::tab::{HistoryEntry, TabContent, TabState};

/// Top-level application model.
pub struct AppModel {
    pub tabs: Vec<TabState>,
    pub active_tab: usize,
    pub url_bar_text: String,
    pub url_bar_focused: bool,
    pub hydra_status: Option<hydra_core::node::HydraStatus>,
    pub hydra_alerts: Vec<hydra_core::alert::AlertResult>,
}

impl AppModel {
    pub fn new() -> Self {
        Self {
            tabs: vec![TabState::new_blank()],
            active_tab: 0,
            url_bar_text: String::new(),
            url_bar_focused: true,
            hydra_status: None,
            hydra_alerts: Vec::new(),
        }
    }

    pub fn active_tab(&self) -> &TabState {
        &self.tabs[self.active_tab]
    }

    pub fn active_tab_mut(&mut self) -> &mut TabState {
        &mut self.tabs[self.active_tab]
    }

    /// Add a new blank tab and switch to it.
    pub fn new_tab(&mut self) {
        self.tabs.push(TabState::new_blank());
        self.active_tab = self.tabs.len() - 1;
        self.url_bar_text.clear();
        self.url_bar_focused = true;
    }

    /// Close tab at index; returns false if it was the last tab.
    pub fn close_tab(&mut self, index: usize) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        self.tabs.remove(index);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.sync_url_bar();
        true
    }

    pub fn switch_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
            self.sync_url_bar();
        }
    }

    fn sync_url_bar(&mut self) {
        self.url_bar_text = self.tabs[self.active_tab].url.clone();
    }

    /// Convert current state to serializable session data.
    pub fn to_session_data(&self) -> SessionData {
        SessionData {
            active_tab: self.active_tab,
            tabs: self
                .tabs
                .iter()
                .map(|tab| SessionTab {
                    url: tab.url.clone(),
                    title: tab.title.clone(),
                    history: tab
                        .history
                        .iter()
                        .map(|h| SessionHistoryEntry {
                            url: h.url.clone(),
                            title: h.title.clone(),
                        })
                        .collect(),
                    history_index: tab.history_index,
                })
                .collect(),
        }
    }

    /// Restore tabs from session data. Content is set to Loading — caller
    /// must trigger navigation to re-fetch each tab.
    pub fn restore_from_session(&mut self, data: SessionData) {
        if data.tabs.is_empty() {
            return;
        }

        self.tabs = data
            .tabs
            .into_iter()
            .map(|st| TabState {
                url: st.url.clone(),
                title: st.title,
                content: if st.url.is_empty() {
                    TabContent::Blank
                } else {
                    TabContent::Loading
                },
                history: st
                    .history
                    .into_iter()
                    .map(|h| HistoryEntry {
                        url: h.url,
                        title: h.title,
                        content: None, // Pages are re-fetched
                    })
                    .collect(),
                history_index: st.history_index,
            })
            .collect();

        self.active_tab = data.active_tab.min(self.tabs.len().saturating_sub(1));
        self.sync_url_bar();
    }
}
