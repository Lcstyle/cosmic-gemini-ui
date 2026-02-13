use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub active_tab: usize,
    pub tabs: Vec<SessionTab>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTab {
    pub url: String,
    pub title: String,
    pub history: Vec<SessionHistoryEntry>,
    pub history_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHistoryEntry {
    pub url: String,
    pub title: String,
}

pub fn session_path() -> PathBuf {
    store::data_dir().join("session.json")
}

/// Save session data atomically (write to tmp, then rename).
pub fn save_session(data: &SessionData) -> Result<(), String> {
    let path = session_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Create dir: {}", e))?;
    }

    let json =
        serde_json::to_string_pretty(data).map_err(|e| format!("Serialize session: {}", e))?;

    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, &json).map_err(|e| format!("Write session: {}", e))?;
    fs::rename(&tmp_path, &path).map_err(|e| format!("Rename session: {}", e))?;

    Ok(())
}

/// Load saved session data, returning None if no session file exists.
pub fn load_session() -> Result<Option<SessionData>, String> {
    let path = session_path();
    if !path.exists() {
        return Ok(None);
    }

    let data = fs::read_to_string(&path).map_err(|e| format!("Read session: {}", e))?;
    let session: SessionData =
        serde_json::from_str(&data).map_err(|e| format!("Parse session: {}", e))?;

    Ok(Some(session))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let data = SessionData {
            active_tab: 1,
            tabs: vec![
                SessionTab {
                    url: "gemini://example.com/".to_string(),
                    title: "Example".to_string(),
                    history: vec![SessionHistoryEntry {
                        url: "gemini://example.com/".to_string(),
                        title: "Example".to_string(),
                    }],
                    history_index: Some(0),
                },
                SessionTab {
                    url: "gemini://other.com/".to_string(),
                    title: "Other".to_string(),
                    history: vec![],
                    history_index: None,
                },
            ],
        };

        save_session(&data).unwrap();
        let loaded = load_session().unwrap().unwrap();
        assert_eq!(loaded.active_tab, 1);
        assert_eq!(loaded.tabs.len(), 2);
        assert_eq!(loaded.tabs[0].url, "gemini://example.com/");
        assert_eq!(loaded.tabs[1].title, "Other");

        // Cleanup
        let _ = std::fs::remove_file(session_path());
    }
}
