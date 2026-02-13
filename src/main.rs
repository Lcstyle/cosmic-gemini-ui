mod app;
mod config;
mod menu;
mod message;
mod model;
mod tab;
mod update;
mod views;

use anyhow::Result;
use cosmic::app::Settings;

fn main() -> Result<()> {
    env_logger::init();
    log::info!("cosmic-gemini starting");

    let url = std::env::args().nth(1);
    let flags = app::Flags { url };

    // Workaround: libcosmic v1.0.0 has a RwLock self-deadlock in FAMILY_MAP.
    // The From<FontConfig> impl holds a read lock and tries to acquire a write
    // lock on the same RwLock when the font family isn't cached (always on first
    // call since the set starts empty). Pre-populating avoids the write path.
    // Upstream bug: Mutex→RwLock refactor in libcosmic src/config/mod.rs.
    {
        let iface = cosmic::config::interface_font();
        let mono = cosmic::config::monospace_font();
        let mut map = cosmic::config::FAMILY_MAP.write().unwrap();
        map.insert(Box::leak(iface.family.into_boxed_str()));
        map.insert(Box::leak(mono.family.into_boxed_str()));
    }

    let settings = Settings::default();

    cosmic::app::run::<app::App>(settings, flags)
        .map_err(|e| anyhow::anyhow!(e))
}
