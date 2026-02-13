use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};

pub const APP_ID: &str = "com.cosmic_gemini.app";

#[derive(Debug, Clone, CosmicConfigEntry, PartialEq)]
#[version = 1]
pub struct AppConfig {
    pub font_size: u16,
    pub max_content_width: u16,
    pub search_engine: String,
    pub auto_load_images: bool,
    pub home_page: String,
    pub hydra_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            font_size: 16,
            max_content_width: 800,
            search_engine: "gemini://tlgs.one/search?%s".to_string(),
            auto_load_images: true,
            home_page: String::new(),
            hydra_enabled: false,
        }
    }
}
