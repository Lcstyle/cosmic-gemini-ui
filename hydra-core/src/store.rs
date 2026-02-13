use std::fs;
use std::path::PathBuf;

use crate::error::HydraError;

/// Return the HYDRA data directory: `~/.local/share/cosmic-gemini/hydra/`
pub fn hydra_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cosmic-gemini")
        .join("hydra")
}

/// Return the Tor state directory: `~/.local/share/cosmic-gemini/hydra/tor/`
pub fn tor_data_dir() -> PathBuf {
    hydra_data_dir().join("tor")
}

/// Ensure the HYDRA data directory exists.
pub fn ensure_dir() -> Result<(), HydraError> {
    let dir = hydra_data_dir();
    fs::create_dir_all(&dir)?;
    Ok(())
}

/// Write data atomically: write to `.tmp`, then rename.
pub fn atomic_write(path: &std::path::Path, data: &[u8]) -> Result<(), HydraError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, data)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}
