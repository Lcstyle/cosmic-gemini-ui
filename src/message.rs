use std::sync::Arc;

use cosmic::widget::segmented_button;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AppMessage {
    // Navigation
    Navigate(String),
    GoHome,
    Back,
    Forward,
    Reload,

    // URL bar
    UrlBarChanged(String),
    FocusUrlBar,

    // Tabs
    NewTab,
    CloseTab(usize),
    SwitchTab(usize),
    NextTab,
    PrevTab,

    // Tab bar (segmented button)
    TabActivate(segmented_button::Entity),
    TabClose(segmented_button::Entity),
    TabContext(segmented_button::Entity),
    ContextCloseTab,
    ContextBookmarkTab,

    // Page load results
    PageLoaded(Result<PageContent, String>),

    // Link clicks
    LinkClicked(String),

    // Input prompt (status 1x)
    InputSubmitted(String),
    InputChanged(String),

    // Bookmarks
    ToggleBookmark,

    // Certificate / TOFU
    TrustCertificate(String), // URL to retry after trusting
    CertWarning { url: String, error: String },

    // Downloads
    DownloadStarted { url: String, filename: String },
    DownloadProgress { filename: String, bytes: u64 },
    DownloadComplete { filename: String, path: String },
    DownloadFailed(String),
    OpenDownload(String), // path

    // Identity management
    IdentityRequired { url: String },
    SelectIdentity { url: String, identity_id: String },
    CreateIdentity { name: String, duration_days: u64 },
    ImportIdentity { name: String, cert_pem: String, key_pem: String },
    IdentityNameChanged(String),
    DeleteIdentity(String),
    IdentityCreated(Result<String, String>),
    BindIdentityToHost { hostname: String, identity_id: String },
    UnbindHost(String),
    ShowIdentityManager,

    // Titan upload
    TitanUploadRequested(String),
    TitanTextChanged(String),
    TitanTokenChanged(String),
    TitanMimeChanged(String),
    TitanSubmit,
    TitanUploadResult(Result<PageContent, String>),

    // Misfin messaging
    MisfinComposeRequested(String),
    MisfinMessageChanged(String),
    MisfinIdentitySelected(String),
    MisfinSend,
    MisfinResult(Result<String, String>),

    // Inline images
    ImageLoaded { tab_index: usize, block_index: usize, data: Vec<u8> },
    ImageFailed { tab_index: usize, block_index: usize, error: String },

    // Session persistence
    SaveSession,
    SessionLoaded(gemini_core::session::SessionData),

    // HYDRA protocol
    HydraNodeStarted(Option<Arc<hydra_core::node::HydraHandle>>),
    HydraStatusUpdate(hydra_core::node::HydraStatus),
    HydraAlert(hydra_core::alert::AlertResult),
    HydraObservationRecorded(String),
    HydraSyncComplete { peer_id: String, events_exchanged: usize },
    HydraError(String),
    ShowHydraPanel,
    HydraToggleEnabled,
    HydraAddPeer(String),
    HydraRemovePeer(String),
    HydraManualSync,
    HydraDismissAlert(usize),
    HydraPeerAddressChanged(String),

    // Internal
    NoOp,
}

/// Parsed page content ready for display.
#[derive(Debug, Clone)]
pub struct PageContent {
    pub url: String,
    pub status: PageStatus,
    pub meta: String,
    pub body: Option<String>,
}

#[derive(Debug, Clone)]
pub enum PageStatus {
    Success,
    Input { sensitive: bool },
    Redirect(String),
    TempFail,
    PermFail,
    CertRequired,
}
