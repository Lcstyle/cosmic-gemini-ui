/// Represents a single block of rendered gemtext content.
#[derive(Debug, Clone)]
pub enum Block {
    Heading { level: u8, text: String },
    Paragraph(String),
    Link { href: String, label: String },
    PreBlock { lines: Vec<String> },
    Quote(String),
    ListItem(String),
    Image {
        alt: String,
        url: String,
        data: Option<Vec<u8>>,
    },
    Blank,
}

/// Current content state for a tab.
#[derive(Debug, Clone)]
pub enum TabContent {
    Loading,
    Document(Vec<Block>),
    Error(String),
    Input { prompt: String, sensitive: bool, value: String },
    CertWarning { url: String, error: String },
    Download { filename: String, bytes_downloaded: u64, complete: bool, path: Option<String> },
    IdentityRequired {
        url: String,
        identities: Vec<(String, String, String)>, // (id, name, fingerprint)
    },
    IdentityManager {
        identities: Vec<(String, String, String, String, String)>, // (id, name, fingerprint, created, expires)
        bindings: Vec<(String, Vec<String>)>, // (identity_id, hostnames)
        new_identity_name: String, // text input for new identity name
    },
    TitanUpload {
        url: String,
        mime: String,
        text: String,
        token: String,
    },
    MisfinCompose {
        recipient: String,
        message: String,
        identity_id: Option<String>,
        char_count: usize,
        identities: Vec<(String, String)>, // (id, name) — cached for view
    },
    MisfinSent {
        recipient: String,
        status: String,
    },
    HydraPanel {
        new_peer_address: String,
    },
    Blank,
}

/// Per-tab navigation history entry.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub content: Option<Vec<Block>>,
}

/// State of a single browser tab.
#[derive(Debug, Clone)]
pub struct TabState {
    pub url: String,
    pub title: String,
    pub content: TabContent,
    pub history: Vec<HistoryEntry>,
    pub history_index: Option<usize>,
}

impl TabState {
    pub fn new_blank() -> Self {
        Self {
            url: String::new(),
            title: "New Tab".to_string(),
            content: TabContent::Blank,
            history: Vec::new(),
            history_index: None,
        }
    }

    pub fn can_go_back(&self) -> bool {
        matches!(self.history_index, Some(i) if i > 0)
    }

    pub fn can_go_forward(&self) -> bool {
        match self.history_index {
            Some(i) => i + 1 < self.history.len(),
            None => false,
        }
    }

    /// Push a new URL onto history, truncating any forward entries.
    pub fn push_history(&mut self, url: &str, title: &str, content: Option<Vec<Block>>) {
        let new_index = match self.history_index {
            Some(i) => {
                self.history.truncate(i + 1);
                i + 1
            }
            None => 0,
        };
        self.history.push(HistoryEntry {
            url: url.to_string(),
            title: title.to_string(),
            content,
        });
        self.history_index = Some(new_index);
    }

    /// Navigate back; returns the URL to load (or None).
    pub fn go_back(&mut self) -> Option<&HistoryEntry> {
        if let Some(i) = self.history_index {
            if i > 0 {
                self.history_index = Some(i - 1);
                return self.history.get(i - 1);
            }
        }
        None
    }

    /// Navigate forward; returns the URL to load (or None).
    pub fn go_forward(&mut self) -> Option<&HistoryEntry> {
        if let Some(i) = self.history_index {
            if i + 1 < self.history.len() {
                self.history_index = Some(i + 1);
                return self.history.get(i + 1);
            }
        }
        None
    }
}
