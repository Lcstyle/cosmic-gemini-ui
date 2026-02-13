use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub static DEFAULT_BOOKMARKS: &str = r#"# Bookmarks

This is a Gemini file where you can put all your bookmarks.
You can edit this file in a text editor to manage them.

## Default bookmarks

=> gemini://geminiprotocol.net Gemini Project
=> gemini://warmedal.se/~antenna/ Antenna aggregator
=> gemini://tlgs.one/ TLGS search engine

## Custom bookmarks

"#;

/// Get the data directory for cosmic-gemini.
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cosmic-gemini")
}

/// Get the config directory for cosmic-gemini.
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cosmic-gemini")
}

/// Get the download directory for cosmic-gemini.
pub fn download_dir() -> PathBuf {
    dirs::download_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("cosmic-gemini")
}

pub fn bookmarks_path() -> PathBuf {
    data_dir().join("bookmarks.gemini")
}

pub fn history_path() -> PathBuf {
    data_dir().join("history.gemini")
}

/// Load bookmarks, creating default file if it doesn't exist.
pub fn load_bookmarks() -> String {
    let path = bookmarks_path();
    if path.exists() {
        fs::read_to_string(&path).unwrap_or_else(|_| DEFAULT_BOOKMARKS.to_string())
    } else {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&path, DEFAULT_BOOKMARKS);
        DEFAULT_BOOKMARKS.to_string()
    }
}

/// Append a bookmark to the bookmarks file.
pub fn add_bookmark(url: &str, title: &str) {
    let path = bookmarks_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = fs::OpenOptions::new().append(true).create(true).open(&path) {
        let _ = writeln!(file, "=> {} {}", url, title);
    }
}

/// Append a URL to the history file.
pub fn add_history(url: &str, title: &str) {
    let path = history_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = fs::OpenOptions::new().append(true).create(true).open(&path) {
        let _ = writeln!(file, "=> {} {}", url, title);
    }
}

/// Load history as a gemtext string.
pub fn load_history() -> String {
    let path = history_path();
    if path.exists() {
        fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    }
}
