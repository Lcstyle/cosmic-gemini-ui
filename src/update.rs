use cosmic::{Action, Task};
use tokio::sync::broadcast;
use url::Url;

use crate::app::App;
use crate::message::{AppMessage, PageContent, PageStatus};
use crate::tab::{Block, TabContent};

/// Parse gemtext body into display blocks using gemini_core parser.
pub fn parse_gemtext(body: &str, base_url: &str) -> Vec<Block> {
    let mut parser = gemini_core::parser::Parser::new();
    let mut blocks = Vec::new();
    let mut pre_lines: Option<Vec<String>> = None;

    for line in body.lines() {
        let mut events = Vec::new();
        parser.parse_line(line, &mut events);

        for event in events {
            match event {
                gemini_core::parser::Event::Start(tag) => match tag {
                    gemini_core::parser::Tag::CodeBlock => {
                        pre_lines = Some(Vec::new());
                    }
                    gemini_core::parser::Tag::Heading(level) => {
                        // Text will follow
                        blocks.push(Block::Heading {
                            level,
                            text: String::new(),
                        });
                    }
                    gemini_core::parser::Tag::Paragraph => {
                        blocks.push(Block::Paragraph(String::new()));
                    }
                    gemini_core::parser::Tag::BlockQuote => {}
                    gemini_core::parser::Tag::UnorderedList => {}
                    gemini_core::parser::Tag::Item => {
                        blocks.push(Block::ListItem(String::new()));
                    }
                    gemini_core::parser::Tag::Link(href, label) => {
                        let resolved = resolve_url(base_url, &href);
                        let display = label.unwrap_or_else(|| resolved.clone());
                        if is_image_url(&resolved) {
                            blocks.push(Block::Image {
                                alt: display,
                                url: resolved,
                                data: None,
                            });
                        } else {
                            blocks.push(Block::Link {
                                href: resolved,
                                label: display,
                            });
                        }
                    }
                },
                gemini_core::parser::Event::Text(text) => {
                    if let Some(ref mut lines) = pre_lines {
                        lines.push(text.to_string());
                    } else {
                        // Attach text to the last block
                        match blocks.last_mut() {
                            Some(Block::Heading { text: t, .. }) => {
                                *t = text.to_string();
                            }
                            Some(Block::Paragraph(t)) => {
                                *t = text.to_string();
                            }
                            Some(Block::ListItem(t)) => {
                                *t = text.to_string();
                            }
                            Some(Block::Quote(t)) => {
                                if !t.is_empty() {
                                    t.push('\n');
                                }
                                t.push_str(text);
                            }
                            _ => {
                                // Quote text — check if we're in a blockquote context
                                blocks.push(Block::Quote(text.to_string()));
                            }
                        }
                    }
                }
                gemini_core::parser::Event::End => {
                    if let Some(lines) = pre_lines.take() {
                        blocks.push(Block::PreBlock { lines });
                    }
                }
                gemini_core::parser::Event::BlankLine => {
                    blocks.push(Block::Blank);
                }
            }
        }
    }

    // Close any remaining pre block
    if let Some(lines) = pre_lines.take() {
        blocks.push(Block::PreBlock { lines });
    }

    blocks
}

/// Resolve a potentially relative URL against a base URL.
fn resolve_url(base: &str, href: &str) -> String {
    if let Ok(url) = Url::parse(href) {
        return url.to_string();
    }
    if let Ok(base_url) = Url::parse(base) {
        if let Ok(resolved) = base_url.join(href) {
            return resolved.to_string();
        }
    }
    href.to_string()
}

/// Get a cached list of identity (id, name) pairs for views.
fn identity_name_list() -> Vec<(String, String)> {
    gemini_core::identity::list_identities()
        .into_iter()
        .map(|i| (i.id, i.name))
        .collect()
}

/// Check if a URL points to a known image format by extension.
fn is_image_url(href: &str) -> bool {
    let lower = href.to_lowercase();
    // Strip query/fragment before checking extension
    let path = lower.split('?').next().unwrap_or(&lower);
    let path = path.split('#').next().unwrap_or(path);
    path.ends_with(".png")
        || path.ends_with(".jpg")
        || path.ends_with(".jpeg")
        || path.ends_with(".gif")
        || path.ends_with(".webp")
}

/// Normalize user input into a proper URL.
pub fn normalize_url(input: &str) -> String {
    let trimmed = input.trim();

    // Already has a scheme (gemini://, titan://, misfin://, http://, etc.)
    if trimmed.contains("://") {
        return trimmed.to_string();
    }

    // Looks like a domain (contains a dot with alphanumeric segments)
    if trimmed.contains('.') && !trimmed.contains(' ') {
        return format!("gemini://{}", trimmed);
    }

    // Treat as search query
    format!("gemini://tlgs.one/search?{}", urlencoding_encode(trimmed))
}

fn urlencoding_encode(s: &str) -> String {
    let mut result = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

/// Handle an AppMessage and return a Task.
pub fn update(app: &mut App, message: &AppMessage) -> Task<Action<AppMessage>> {
    let cert_obs = app.cert_observer_tx.clone();
    match message {
        AppMessage::Navigate(input) => {
            let url = normalize_url(input);
            app.model.url_bar_text = url.clone();
            app.model.active_tab_mut().url = url.clone();

            // Handle titan:// URLs
            if url.starts_with("titan://") {
                app.model.active_tab_mut().content = TabContent::TitanUpload {
                    url: url.clone(),
                    mime: "text/gemini".to_string(),
                    text: String::new(),
                    token: String::new(),
                };
                app.model.active_tab_mut().title = "Titan Upload".to_string();
                return Task::none();
            }

            // Handle misfin:// URLs
            if url.starts_with("misfin://") {
                let recipient = url.trim_start_matches("misfin://").to_string();
                let idents = identity_name_list();
                app.model.active_tab_mut().content = TabContent::MisfinCompose {
                    recipient,
                    message: String::new(),
                    identity_id: None,
                    char_count: 0,
                    identities: idents,
                };
                app.model.active_tab_mut().title = "Misfin Message".to_string();
                return Task::none();
            }

            app.model.active_tab_mut().content = TabContent::Loading;
            return spawn_fetch(url, cert_obs.clone());
        }

        AppMessage::LinkClicked(href) => {
            // Resolve relative URLs against the current page URL
            let resolved = resolve_url(&app.model.active_tab().url, href);

            // Check if it's a non-gemini URL
            if let Ok(parsed) = Url::parse(&resolved) {
                match parsed.scheme() {
                    "gemini" => {}
                    "titan" => {
                        return update(app, &AppMessage::Navigate(resolved));
                    }
                    "misfin" => {
                        return update(app, &AppMessage::Navigate(resolved));
                    }
                    _ => {
                        let _ = open::that(&resolved);
                        return Task::none();
                    }
                }
            }

            app.model.url_bar_text = resolved.clone();
            app.model.active_tab_mut().url = resolved.clone();
            app.model.active_tab_mut().content = TabContent::Loading;
            return spawn_fetch(resolved, cert_obs.clone());
        }

        AppMessage::GoHome => {
            if !app.config.home_page.is_empty() {
                let url = app.config.home_page.clone();
                return update(app, &AppMessage::Navigate(url));
            }
            load_bookmarks_into_active_tab(app);
        }

        AppMessage::Back => {
            let entry = app.model.active_tab_mut().go_back().cloned();
            if let Some(entry) = entry {
                app.model.url_bar_text = entry.url.clone();
                app.model.active_tab_mut().url = entry.url.clone();
                if let Some(content) = entry.content {
                    app.model.active_tab_mut().content = TabContent::Document(content);
                    app.model.active_tab_mut().title = entry.title;
                } else {
                    app.model.active_tab_mut().content = TabContent::Loading;
                    return spawn_fetch(entry.url, cert_obs.clone());
                }
            }
        }

        AppMessage::Forward => {
            let entry = app.model.active_tab_mut().go_forward().cloned();
            if let Some(entry) = entry {
                app.model.url_bar_text = entry.url.clone();
                app.model.active_tab_mut().url = entry.url.clone();
                if let Some(content) = entry.content {
                    app.model.active_tab_mut().content = TabContent::Document(content);
                    app.model.active_tab_mut().title = entry.title;
                } else {
                    app.model.active_tab_mut().content = TabContent::Loading;
                    return spawn_fetch(entry.url, cert_obs.clone());
                }
            }
        }

        AppMessage::Reload => {
            let url = app.model.active_tab().url.clone();
            if !url.is_empty() {
                app.model.active_tab_mut().content = TabContent::Loading;
                return spawn_fetch(url, cert_obs.clone());
            }
        }

        AppMessage::UrlBarChanged(text) => {
            app.model.url_bar_text = text.clone();
        }

        AppMessage::FocusUrlBar => {
            app.model.url_bar_focused = true;
        }

        AppMessage::NewTab => {
            app.model.new_tab();
            if !app.config.home_page.is_empty() {
                let url = app.config.home_page.clone();
                app.model.url_bar_text = url.clone();
                app.model.active_tab_mut().url = url.clone();
                app.model.active_tab_mut().content = TabContent::Loading;
                app.rebuild_tab_model();
                return spawn_fetch(url, cert_obs.clone());
            }
            load_bookmarks_into_active_tab(app);
            app.rebuild_tab_model();
        }

        AppMessage::CloseTab(index) => {
            let idx = if *index == usize::MAX {
                app.model.active_tab
            } else {
                *index
            };
            if !app.model.close_tab(idx) {
                // Last tab — reset to blank with bookmarks
                app.model.active_tab_mut().url.clear();
                app.model.active_tab_mut().title = "New Tab".to_string();
                app.model.url_bar_text.clear();
                load_bookmarks_into_active_tab(app);
            }
            app.rebuild_tab_model();
        }

        AppMessage::SwitchTab(index) => {
            app.model.switch_tab(*index);
            app.rebuild_tab_model();
        }

        AppMessage::NextTab => {
            let next = (app.model.active_tab + 1) % app.model.tabs.len();
            app.model.switch_tab(next);
            app.rebuild_tab_model();
        }

        AppMessage::PrevTab => {
            let len = app.model.tabs.len();
            let prev = (app.model.active_tab + len - 1) % len;
            app.model.switch_tab(prev);
            app.rebuild_tab_model();
        }

        AppMessage::TabActivate(entity) => {
            if let Some(pos) = app.tab_model.position(*entity) {
                app.model.switch_tab(pos as usize);
                app.tab_model.activate(*entity);
            }
        }

        AppMessage::TabClose(entity) => {
            if let Some(pos) = app.tab_model.position(*entity) {
                let idx = pos as usize;
                if !app.model.close_tab(idx) {
                    // Last tab — reset to blank with bookmarks
                    app.model.active_tab_mut().url.clear();
                    app.model.active_tab_mut().title = "New Tab".to_string();
                    app.model.url_bar_text.clear();
                    load_bookmarks_into_active_tab(app);
                }
                app.rebuild_tab_model();
            }
        }

        AppMessage::TabContext(entity) => {
            app.context_tab = Some(*entity);
        }

        AppMessage::ContextCloseTab => {
            if let Some(entity) = app.context_tab.take() {
                if let Some(pos) = app.tab_model.position(entity) {
                    let idx = pos as usize;
                    if !app.model.close_tab(idx) {
                        app.model.active_tab_mut().url.clear();
                        app.model.active_tab_mut().title = "New Tab".to_string();
                        app.model.url_bar_text.clear();
                        load_bookmarks_into_active_tab(app);
                    }
                    app.rebuild_tab_model();
                }
            }
        }

        AppMessage::ContextBookmarkTab => {
            if let Some(entity) = app.context_tab.take() {
                if let Some(pos) = app.tab_model.position(entity) {
                    let idx = pos as usize;
                    if let Some(tab) = app.model.tabs.get(idx) {
                        if !tab.url.is_empty() {
                            gemini_core::store::add_bookmark(&tab.url, &tab.title);
                        }
                    }
                }
            }
        }

        AppMessage::PageLoaded(result) => {
            match result {
                Ok(content) => {
                    let url = content.url.clone();
                    match &content.status {
                        PageStatus::Success => {
                            if let Some(body) = &content.body {
                                let blocks = parse_gemtext(body, &url);
                                let title = extract_title(&blocks)
                                    .unwrap_or_else(|| url.clone());

                                app.model.active_tab_mut().title = title.clone();
                                app.model.active_tab_mut().push_history(
                                    &url,
                                    &title,
                                    Some(blocks.clone()),
                                );
                                app.model.active_tab_mut().content =
                                    TabContent::Document(blocks);

                                app.rebuild_tab_model();
                                gemini_core::store::add_history(&url, &title);

                                // Spawn async image fetches for inline images
                                if app.config.auto_load_images {
                                    let tab_index = app.model.active_tab;
                                    let image_tasks = spawn_image_fetches(tab_index, app, cert_obs.clone());
                                    if !image_tasks.is_empty() {
                                        return Task::batch(image_tasks);
                                    }
                                }
                            }
                        }
                        PageStatus::Input { sensitive } => {
                            app.model.active_tab_mut().content = TabContent::Input {
                                prompt: content.meta.clone(),
                                sensitive: *sensitive,
                                value: String::new(),
                            };
                        }
                        PageStatus::Redirect(target) => {
                            let resolved = resolve_url(&url, target);
                            app.model.url_bar_text = resolved.clone();
                            app.model.active_tab_mut().url = resolved.clone();
                            app.model.active_tab_mut().content = TabContent::Loading;
                            // Preserve bound identity across redirects
                            if let Ok(parsed) = Url::parse(&resolved) {
                                if let Some(host) = parsed.host_str() {
                                    if let Some(bound_id) =
                                        gemini_core::identity::get_host_binding(host)
                                    {
                                        return spawn_fetch_with_identity(resolved, bound_id, cert_obs.clone());
                                    }
                                }
                            }
                            return spawn_fetch(resolved, cert_obs.clone());
                        }
                        PageStatus::TempFail | PageStatus::PermFail => {
                            app.model.active_tab_mut().content = TabContent::Error(
                                format!("Server error: {}", content.meta),
                            );
                        }
                        PageStatus::CertRequired => {
                            // Check for auto-bound identity
                            if let Ok(parsed) = Url::parse(&url) {
                                if let Some(host) = parsed.host_str() {
                                    if let Some(bound_id) = gemini_core::identity::get_host_binding(host) {
                                        // Auto-present bound identity
                                        return spawn_fetch_with_identity(url, bound_id, cert_obs.clone());
                                    }
                                }
                            }
                            // No auto-binding — show identity selector
                            let identities: Vec<(String, String, String)> =
                                gemini_core::identity::list_identities()
                                    .into_iter()
                                    .map(|i| (i.id, i.name, i.fingerprint))
                                    .collect();
                            app.model.active_tab_mut().content = TabContent::IdentityRequired {
                                url: url.clone(),
                                identities,
                            };
                        }
                    }
                }
                Err(err) => {
                    app.model.active_tab_mut().content =
                        TabContent::Error(err.clone());
                }
            }
        }

        AppMessage::InputSubmitted(value) => {
            let base_url = app.model.active_tab().url.clone();
            if let Ok(mut url) = Url::parse(&base_url) {
                url.set_query(Some(value));
                let url_str = url.to_string();
                app.model.url_bar_text = url_str.clone();
                app.model.active_tab_mut().url = url_str.clone();
                app.model.active_tab_mut().content = TabContent::Loading;
                // Use bound identity if the host requires client cert
                if let Some(host) = url.host_str() {
                    if let Some(bound_id) =
                        gemini_core::identity::get_host_binding(host)
                    {
                        return spawn_fetch_with_identity(url_str, bound_id, cert_obs.clone());
                    }
                }
                return spawn_fetch(url_str, cert_obs.clone());
            }
        }

        AppMessage::InputChanged(value) => {
            if let TabContent::Input { value: v, .. } =
                &mut app.model.active_tab_mut().content
            {
                *v = value.clone();
            }
        }

        AppMessage::ToggleBookmark => {
            let tab = app.model.active_tab();
            if !tab.url.is_empty() {
                gemini_core::store::add_bookmark(&tab.url, &tab.title);
            }
        }

        AppMessage::CertWarning { url, error } => {
            app.model.active_tab_mut().content = TabContent::CertWarning {
                url: url.clone(),
                error: error.clone(),
            };
        }

        AppMessage::TrustCertificate(url) => {
            // Remove the old known_hosts entry so TOFU re-pins
            if let Ok(parsed) = Url::parse(&url) {
                if let Some(host) = parsed.host_str() {
                    if let Ok(mut kh) =
                        gemini_core::known_hosts::KnownHostsFile::open_default()
                    {
                        use gemini_core::known_hosts::KnownHostsRepo;
                        kh.remove(host);
                    }
                }
            }
            // Retry the fetch
            app.model.active_tab_mut().content = TabContent::Loading;
            return spawn_fetch(url.clone(), cert_obs.clone());
        }

        AppMessage::DownloadStarted { filename, .. } => {
            app.model.active_tab_mut().content = TabContent::Download {
                filename: filename.clone(),
                bytes_downloaded: 0,
                complete: false,
                path: None,
            };
        }

        AppMessage::DownloadProgress { filename, bytes } => {
            if let TabContent::Download {
                filename: f,
                bytes_downloaded,
                ..
            } = &mut app.model.active_tab_mut().content
            {
                *f = filename.clone();
                *bytes_downloaded = *bytes;
            }
        }

        AppMessage::DownloadComplete { filename, path } => {
            app.model.active_tab_mut().content = TabContent::Download {
                filename: filename.clone(),
                bytes_downloaded: 0,
                complete: true,
                path: Some(path.clone()),
            };
        }

        AppMessage::DownloadFailed(err) => {
            app.model.active_tab_mut().content =
                TabContent::Error(format!("Download failed: {}", err));
        }

        AppMessage::OpenDownload(path) => {
            let _ = open::that(path);
        }

        // Identity management
        AppMessage::IdentityRequired { url } => {
            let identities: Vec<(String, String, String)> =
                gemini_core::identity::list_identities()
                    .into_iter()
                    .map(|i| (i.id, i.name, i.fingerprint))
                    .collect();
            app.model.active_tab_mut().content = TabContent::IdentityRequired {
                url: url.clone(),
                identities,
            };
        }

        AppMessage::SelectIdentity { url, identity_id } => {
            // Bind identity to host for future auto-presentation
            if let Ok(parsed) = Url::parse(url) {
                if let Some(host) = parsed.host_str() {
                    let _ = gemini_core::identity::bind_host(host, identity_id);
                }
            }
            app.model.active_tab_mut().content = TabContent::Loading;
            return spawn_fetch_with_identity(url.clone(), identity_id.clone(), cert_obs.clone());
        }

        AppMessage::CreateIdentity { name, duration_days } => {
            match gemini_core::identity::generate_identity(name, *duration_days) {
                Ok(identity) => {
                    log::info!("Created identity: {} ({})", identity.name, identity.id);
                    // Refresh identity manager or selector if active
                    return update(app, &AppMessage::IdentityCreated(Ok(identity.id)));
                }
                Err(e) => {
                    log::error!("Identity creation failed: {}", e);
                    return update(app, &AppMessage::IdentityCreated(Err(e)));
                }
            }
        }

        AppMessage::ImportIdentity { name, cert_pem, key_pem } => {
            match gemini_core::identity::import_identity(name, cert_pem, key_pem) {
                Ok(identity) => {
                    return update(app, &AppMessage::IdentityCreated(Ok(identity.id)));
                }
                Err(e) => {
                    return update(app, &AppMessage::IdentityCreated(Err(e)));
                }
            }
        }

        AppMessage::DeleteIdentity(id) => {
            if let Err(e) = gemini_core::identity::delete_identity(id) {
                log::error!("Delete identity failed: {}", e);
            }
            // Refresh manager view
            return update(app, &AppMessage::ShowIdentityManager);
        }

        AppMessage::IdentityCreated(result) => {
            match result {
                Ok(_id) => {
                    // If we're on an IdentityRequired page, refresh the identity list
                    let current_url = match &app.model.active_tab().content {
                        TabContent::IdentityRequired { url, .. } => Some(url.clone()),
                        _ => None,
                    };
                    if let Some(url) = current_url {
                        let identities: Vec<(String, String, String)> =
                            gemini_core::identity::list_identities()
                                .into_iter()
                                .map(|i| (i.id, i.name, i.fingerprint))
                                .collect();
                        app.model.active_tab_mut().content = TabContent::IdentityRequired {
                            url,
                            identities,
                        };
                    } else {
                        // Refresh manager view
                        return update(app, &AppMessage::ShowIdentityManager);
                    }
                }
                Err(e) => {
                    app.model.active_tab_mut().content =
                        TabContent::Error(format!("Identity error: {}", e));
                }
            }
        }

        AppMessage::BindIdentityToHost { hostname, identity_id } => {
            if let Err(e) = gemini_core::identity::bind_host(hostname, identity_id) {
                log::error!("Bind host failed: {}", e);
            }
            return update(app, &AppMessage::ShowIdentityManager);
        }

        AppMessage::UnbindHost(hostname) => {
            if let Err(e) = gemini_core::identity::unbind_host(hostname) {
                log::error!("Unbind host failed: {}", e);
            }
            return update(app, &AppMessage::ShowIdentityManager);
        }

        AppMessage::IdentityNameChanged(name) => {
            if let TabContent::IdentityManager { new_identity_name, .. } =
                &mut app.model.active_tab_mut().content
            {
                *new_identity_name = name.clone();
            }
        }

        AppMessage::ShowIdentityManager => {
            // Preserve the name input if already on the identity manager
            let existing_name = if let TabContent::IdentityManager { new_identity_name, .. } =
                &app.model.active_tab().content
            {
                new_identity_name.clone()
            } else {
                String::new()
            };

            let all_identities = gemini_core::identity::list_identities();
            let identities: Vec<(String, String, String, String, String)> = all_identities
                .iter()
                .map(|i| {
                    (
                        i.id.clone(),
                        i.name.clone(),
                        i.fingerprint.clone(),
                        i.created_at.clone(),
                        i.expires_at.clone(),
                    )
                })
                .collect();
            let bindings: Vec<(String, Vec<String>)> = all_identities
                .iter()
                .map(|i| {
                    let hosts = gemini_core::identity::get_bindings_for_identity(&i.id);
                    (i.id.clone(), hosts)
                })
                .collect();
            app.model.active_tab_mut().content = TabContent::IdentityManager {
                identities,
                bindings,
                new_identity_name: existing_name,
            };
            app.model.active_tab_mut().title = "Identity Manager".to_string();
        }

        // Titan upload
        AppMessage::TitanUploadRequested(url) => {
            app.model.active_tab_mut().content = TabContent::TitanUpload {
                url: url.clone(),
                mime: "text/gemini".to_string(),
                text: String::new(),
                token: String::new(),
            };
            app.model.active_tab_mut().title = "Titan Upload".to_string();
        }

        AppMessage::TitanTextChanged(text) => {
            if let TabContent::TitanUpload { text: t, .. } = &mut app.model.active_tab_mut().content {
                *t = text.clone();
            }
        }

        AppMessage::TitanTokenChanged(token) => {
            if let TabContent::TitanUpload { token: t, .. } = &mut app.model.active_tab_mut().content {
                *t = token.clone();
            }
        }

        AppMessage::TitanMimeChanged(mime) => {
            if let TabContent::TitanUpload { mime: m, .. } = &mut app.model.active_tab_mut().content {
                *m = mime.clone();
            }
        }

        AppMessage::TitanSubmit => {
            if let TabContent::TitanUpload { url, mime, text, token } = app.model.active_tab_mut().content.clone() {
                app.model.active_tab_mut().content = TabContent::Loading;
                let titan_url = url.clone();
                let titan_mime = mime;
                let titan_text = text;
                let titan_token = if token.is_empty() { None } else { Some(token) };

                // Check if we have a bound identity for this host
                let bound_identity: Option<String> = Url::parse(&titan_url)
                    .ok()
                    .and_then(|parsed| {
                        parsed.host_str().and_then(gemini_core::identity::get_host_binding)
                    });

                return Task::future(async move {
                    let request = gemini_core::titan::TitanRequest {
                        url: titan_url.clone(),
                        mime: titan_mime,
                        token: titan_token,
                        payload: titan_text.into_bytes(),
                    };

                    let result = if let Some(identity_id) = bound_identity {
                        // Upload with client certificate
                        match gemini_core::identity::load_identity(&identity_id) {
                            Ok((certs, key)) => {
                                gemini_core::titan::upload_with_identity(&request, certs, key).await
                            }
                            Err(e) => {
                                return Action::App(AppMessage::PageLoaded(Err(
                                    format!("Titan upload failed: could not load identity: {}", e),
                                )));
                            }
                        }
                    } else {
                        gemini_core::titan::upload(&request).await
                    };

                    match result {
                        Ok(response) => {
                            let status = response.status();
                            let meta = response.meta().to_string();
                            let body = if matches!(status, gemini_core::Status::Success(_)) {
                                response.body_text().await
                            } else {
                                None
                            };
                            let page_status = match status {
                                gemini_core::Status::Success(_) => PageStatus::Success,
                                gemini_core::Status::Redirect(_) => PageStatus::Redirect(meta.clone()),
                                gemini_core::Status::TempFail(_) => PageStatus::TempFail,
                                gemini_core::Status::PermFail(_) => PageStatus::PermFail,
                                gemini_core::Status::CertRequired(_) => PageStatus::CertRequired,
                                gemini_core::Status::Input(_) => PageStatus::Input { sensitive: false },
                            };
                            // Convert titan:// back to gemini:// for the response URL
                            // so redirect resolution uses the correct scheme
                            let response_url = if titan_url.starts_with("titan://") {
                                format!("gemini://{}", &titan_url["titan://".len()..])
                            } else {
                                titan_url
                            };
                            Action::App(AppMessage::PageLoaded(Ok(PageContent {
                                url: response_url,
                                status: page_status,
                                meta,
                                body,
                            })))
                        }
                        Err(e) => {
                            Action::App(AppMessage::PageLoaded(Err(format!("Titan upload failed: {}", e))))
                        }
                    }
                });
            }
        }

        AppMessage::TitanUploadResult(result) => {
            match result {
                Ok(content) => {
                    return update(app, &AppMessage::PageLoaded(Ok(content.clone())));
                }
                Err(e) => {
                    app.model.active_tab_mut().content = TabContent::Error(e.clone());
                }
            }
        }

        // Misfin messaging
        AppMessage::MisfinComposeRequested(url) => {
            let recipient = url.trim_start_matches("misfin://").to_string();
            let idents = identity_name_list();
            app.model.active_tab_mut().content = TabContent::MisfinCompose {
                recipient,
                message: String::new(),
                identity_id: None,
                char_count: 0,
                identities: idents,
            };
            app.model.active_tab_mut().title = "Misfin Message".to_string();
        }

        AppMessage::MisfinMessageChanged(text) => {
            if let TabContent::MisfinCompose { message, char_count, .. } = &mut app.model.active_tab_mut().content {
                *char_count = text.len();
                *message = text.clone();
            }
        }

        AppMessage::MisfinIdentitySelected(id) => {
            if let TabContent::MisfinCompose { identity_id, .. } = &mut app.model.active_tab_mut().content {
                *identity_id = Some(id.clone());
            }
        }

        AppMessage::MisfinSend => {
            if let TabContent::MisfinCompose { recipient, message, identity_id, .. } = app.model.active_tab_mut().content.clone() {
                if let Some(id) = identity_id {
                    let recip = recipient.clone();
                    let msg = message.clone();
                    let ident_id = id;
                    app.model.active_tab_mut().content = TabContent::Loading;
                    return Task::future(async move {
                        let (certs, key) = match gemini_core::identity::load_identity(&ident_id) {
                            Ok(pair) => pair,
                            Err(e) => {
                                return Action::App(AppMessage::MisfinResult(Err(
                                    format!("Failed to load identity: {}", e),
                                )));
                            }
                        };
                        match gemini_core::misfin::send_message(&gemini_core::misfin::MisfinMessage {
                            recipient: recip.clone(),
                            body: msg,
                            sender_cert: certs,
                            sender_key: key,
                        })
                        .await
                        {
                            Ok(resp) => Action::App(AppMessage::MisfinResult(Ok(
                                format!("{} {}", resp.status, resp.meta),
                            ))),
                            Err(e) => Action::App(AppMessage::MisfinResult(Err(e.to_string()))),
                        }
                    });
                }
            }
        }

        AppMessage::MisfinResult(result) => {
            let (recipient, status) = match result {
                Ok(meta) => {
                    let recip = match &app.model.active_tab().content {
                        TabContent::MisfinCompose { recipient, .. } => recipient.clone(),
                        _ => "unknown".to_string(),
                    };
                    (recip, format!("Success: {}", meta))
                }
                Err(e) => {
                    let recip = match &app.model.active_tab().content {
                        TabContent::MisfinCompose { recipient, .. } => recipient.clone(),
                        _ => "unknown".to_string(),
                    };
                    (recip, format!("Error: {}", e))
                }
            };
            app.model.active_tab_mut().content = TabContent::MisfinSent {
                recipient,
                status,
            };
        }

        // Inline images
        AppMessage::ImageLoaded { tab_index, block_index, data } => {
            if let Some(tab) = app.model.tabs.get_mut(*tab_index) {
                if let TabContent::Document(blocks) = &mut tab.content {
                    if let Some(Block::Image { data: d, .. }) = blocks.get_mut(*block_index) {
                        *d = Some(data.clone());
                    }
                }
            }
        }

        AppMessage::ImageFailed { tab_index, block_index, error } => {
            log::warn!("Image load failed (tab={}, block={}): {}", tab_index, block_index, error);
        }

        // Session persistence
        AppMessage::SaveSession => {
            let data = app.model.to_session_data();
            if let Err(e) = gemini_core::session::save_session(&data) {
                log::error!("Failed to save session: {}", e);
            }
        }

        AppMessage::SessionLoaded(data) => {
            app.model.restore_from_session(data.clone());
            app.rebuild_tab_model();
            // Re-fetch the active tab
            let url = app.model.active_tab().url.clone();
            if !url.is_empty() {
                return spawn_fetch(url, cert_obs.clone());
            }
        }

        // HYDRA protocol messages
        AppMessage::HydraNodeStarted(handle) => {
            if let Some(handle) = handle {
                let tx = handle.cmd_tx.clone();
                app.hydra_handle = Some(handle.clone());
                // Request initial status
                tokio::spawn(async move {
                    let _ = tx.send(hydra_core::node::HydraCommand::GetStatus).await;
                });
            }
        }

        AppMessage::HydraStatusUpdate(status) => {
            app.model.hydra_status = Some(status.clone());
        }

        AppMessage::HydraAlert(alert) => {
            app.model.hydra_alerts.push(alert.clone());
        }

        AppMessage::HydraObservationRecorded(domain) => {
            log::debug!("HYDRA observation recorded for: {}", domain);
            // Request status update to refresh counts
            if let Some(ref handle) = app.hydra_handle {
                let tx = handle.cmd_tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(hydra_core::node::HydraCommand::GetStatus).await;
                });
            }
        }

        AppMessage::HydraSyncComplete { peer_id, events_exchanged } => {
            log::info!(
                "HYDRA sync complete with peer {}: {} events exchanged",
                &peer_id[..peer_id.len().min(8)],
                events_exchanged
            );
            // Request status update
            if let Some(ref handle) = app.hydra_handle {
                let tx = handle.cmd_tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(hydra_core::node::HydraCommand::GetStatus).await;
                });
            }
        }

        AppMessage::HydraError(e) => {
            log::error!("HYDRA error: {}", e);
        }

        AppMessage::ShowHydraPanel => {
            // Request fresh status from HYDRA node
            if let Some(ref handle) = app.hydra_handle {
                let tx = handle.cmd_tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(hydra_core::node::HydraCommand::GetStatus).await;
                });
            }

            let existing_address = if let TabContent::HydraPanel { new_peer_address, .. } =
                &app.model.active_tab().content
            {
                new_peer_address.clone()
            } else {
                String::new()
            };

            app.model.active_tab_mut().content = TabContent::HydraPanel {
                new_peer_address: existing_address,
            };
            app.model.active_tab_mut().title = "HYDRA Panel".to_string();
        }

        AppMessage::HydraToggleEnabled => {
            if let Some(ref handle) = app.hydra_handle {
                let tx = handle.cmd_tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(hydra_core::node::HydraCommand::ToggleEnabled).await;
                });
            }
        }

        AppMessage::HydraAddPeer(address) => {
            if let Some(ref handle) = app.hydra_handle {
                let tx = handle.cmd_tx.clone();
                let addr = address.clone();
                tokio::spawn(async move {
                    let _ = tx
                        .send(hydra_core::node::HydraCommand::AddPeer {
                            onion_address: addr,
                        })
                        .await;
                });
            }
            // Clear the peer address input
            if let TabContent::HydraPanel { new_peer_address, .. } =
                &mut app.model.active_tab_mut().content
            {
                new_peer_address.clear();
            }
        }

        AppMessage::HydraRemovePeer(node_id) => {
            if let Some(ref handle) = app.hydra_handle {
                let tx = handle.cmd_tx.clone();
                let id = node_id.clone();
                tokio::spawn(async move {
                    let _ = tx
                        .send(hydra_core::node::HydraCommand::RemovePeer { node_id: id })
                        .await;
                });
            }
        }

        AppMessage::HydraManualSync => {
            if let Some(ref handle) = app.hydra_handle {
                let tx = handle.cmd_tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(hydra_core::node::HydraCommand::ManualSync).await;
                });
            }
        }

        AppMessage::HydraDismissAlert(index) => {
            if *index < app.model.hydra_alerts.len() {
                app.model.hydra_alerts.remove(*index);
            }
        }

        AppMessage::HydraPeerAddressChanged(address) => {
            if let TabContent::HydraPanel { new_peer_address, .. } =
                &mut app.model.active_tab_mut().content
            {
                *new_peer_address = address.clone();
            }
        }

        AppMessage::NoOp => {}
    }

    Task::none()
}

/// Extract the first H1 title from blocks.
fn extract_title(blocks: &[Block]) -> Option<String> {
    for block in blocks {
        if let Block::Heading { level: 1, text } = block {
            if !text.is_empty() {
                return Some(text.clone());
            }
        }
    }
    None
}

/// Load bookmarks into the active tab as a document.
pub fn load_bookmarks_into_active_tab(app: &mut App) {
    let bookmarks = gemini_core::store::load_bookmarks();
    let blocks = parse_gemtext(&bookmarks, "about:bookmarks");
    app.model.active_tab_mut().content = TabContent::Document(blocks);
    app.model.active_tab_mut().title = "Bookmarks".to_string();
}

/// Spawn an async fetch task.
fn spawn_fetch(
    url: String,
    cert_observer: Option<broadcast::Sender<gemini_core::TlsCertCapture>>,
) -> Task<Action<AppMessage>> {
    let fetch_url = url.clone();
    Task::future(async move {
        let mut client = gemini_core::Client::new();
        if let Some(tx) = cert_observer {
            client = client.with_cert_observer(tx);
        }
        match client.fetch(&fetch_url).await {
            Ok(response) => {
                let status = response.status();
                let meta = response.meta().to_string();

                let page_status = match status {
                    gemini_core::Status::Input(10) => PageStatus::Input { sensitive: false },
                    gemini_core::Status::Input(11) => PageStatus::Input { sensitive: true },
                    gemini_core::Status::Input(_) => PageStatus::Input { sensitive: false },
                    gemini_core::Status::Success(_) => PageStatus::Success,
                    gemini_core::Status::Redirect(_) => {
                        PageStatus::Redirect(meta.clone())
                    }
                    gemini_core::Status::TempFail(_) => PageStatus::TempFail,
                    gemini_core::Status::PermFail(_) => PageStatus::PermFail,
                    gemini_core::Status::CertRequired(_) => PageStatus::CertRequired,
                };

                // Handle binary content as downloads
                if matches!(status, gemini_core::Status::Success(_))
                    && !meta.starts_with("text/")
                    && !meta.is_empty()
                {
                    return handle_binary_download(response, &fetch_url, &meta).await;
                }

                let body = if matches!(status, gemini_core::Status::Success(_)) {
                    response.body_text().await
                } else {
                    None
                };

                Action::App(AppMessage::PageLoaded(Ok(PageContent {
                    url: fetch_url,
                    status: page_status,
                    meta,
                    body,
                })))
            }
            Err(e) => {
                // Detect certificate errors for TOFU warning
                let err_str = e.to_string();
                if err_str.contains("changed") || err_str.contains("BadIdentity") {
                    Action::App(AppMessage::CertWarning {
                        url: fetch_url,
                        error: err_str,
                    })
                } else {
                    Action::App(AppMessage::PageLoaded(Err(err_str)))
                }
            }
        }
    })
}

/// Spawn an async fetch task with a client certificate identity.
fn spawn_fetch_with_identity(
    url: String,
    identity_id: String,
    cert_observer: Option<broadcast::Sender<gemini_core::TlsCertCapture>>,
) -> Task<Action<AppMessage>> {
    let fetch_url = url.clone();
    Task::future(async move {
        let (certs, key) = match gemini_core::identity::load_identity(&identity_id) {
            Ok(pair) => pair,
            Err(e) => {
                return Action::App(AppMessage::PageLoaded(Err(
                    format!("Failed to load identity: {}", e),
                )));
            }
        };

        let mut client = gemini_core::Client::new();
        if let Some(tx) = cert_observer {
            client = client.with_cert_observer(tx);
        }
        match client.fetch_with_identity(&fetch_url, certs, key).await {
            Ok(response) => {
                let status = response.status();
                let meta = response.meta().to_string();

                let page_status = match status {
                    gemini_core::Status::Input(10) => PageStatus::Input { sensitive: false },
                    gemini_core::Status::Input(11) => PageStatus::Input { sensitive: true },
                    gemini_core::Status::Input(_) => PageStatus::Input { sensitive: false },
                    gemini_core::Status::Success(_) => PageStatus::Success,
                    gemini_core::Status::Redirect(_) => PageStatus::Redirect(meta.clone()),
                    gemini_core::Status::TempFail(_) => PageStatus::TempFail,
                    gemini_core::Status::PermFail(_) => PageStatus::PermFail,
                    gemini_core::Status::CertRequired(_) => PageStatus::CertRequired,
                };

                if matches!(status, gemini_core::Status::Success(_))
                    && !meta.starts_with("text/")
                    && !meta.is_empty()
                {
                    return handle_binary_download(response, &fetch_url, &meta).await;
                }

                let body = if matches!(status, gemini_core::Status::Success(_)) {
                    response.body_text().await
                } else {
                    None
                };

                Action::App(AppMessage::PageLoaded(Ok(PageContent {
                    url: fetch_url,
                    status: page_status,
                    meta,
                    body,
                })))
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("changed") || err_str.contains("BadIdentity") {
                    Action::App(AppMessage::CertWarning {
                        url: fetch_url,
                        error: err_str,
                    })
                } else {
                    Action::App(AppMessage::PageLoaded(Err(err_str)))
                }
            }
        }
    })
}

/// Spawn async tasks to fetch all inline images in the active tab.
fn spawn_image_fetches(
    tab_index: usize,
    app: &App,
    cert_observer: Option<broadcast::Sender<gemini_core::TlsCertCapture>>,
) -> Vec<Task<Action<AppMessage>>> {
    let mut tasks = Vec::new();

    if let Some(tab) = app.model.tabs.get(tab_index) {
        if let TabContent::Document(blocks) = &tab.content {
            for (block_index, block) in blocks.iter().enumerate() {
                if let Block::Image { url, data: None, .. } = block {
                    let img_url = url.clone();
                    let ti = tab_index;
                    let bi = block_index;
                    let obs = cert_observer.clone();
                    tasks.push(Task::future(async move {
                        match fetch_image_bytes(&img_url, obs).await {
                            Ok(data) => Action::App(AppMessage::ImageLoaded {
                                tab_index: ti,
                                block_index: bi,
                                data,
                            }),
                            Err(e) => Action::App(AppMessage::ImageFailed {
                                tab_index: ti,
                                block_index: bi,
                                error: e,
                            }),
                        }
                    }));
                }
            }
        }
    }

    tasks
}

const MAX_IMAGE_SIZE: usize = 10 * 1024 * 1024; // 10 MB

/// Fetch image bytes from a gemini:// URL.
async fn fetch_image_bytes(
    url: &str,
    cert_observer: Option<broadcast::Sender<gemini_core::TlsCertCapture>>,
) -> Result<Vec<u8>, String> {
    let mut client = gemini_core::Client::new();
    if let Some(tx) = cert_observer {
        client = client.with_cert_observer(tx);
    }
    let response = client.fetch(url).await.map_err(|e| e.to_string())?;

    if !matches!(response.status(), gemini_core::Status::Success(_)) {
        return Err(format!("Image fetch returned status {:?}", response.status()));
    }

    match response.into_body() {
        Some(mut body) => {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            // Read with size limit
            let mut limited = (&mut body).take(MAX_IMAGE_SIZE as u64 + 1);
            limited
                .read_to_end(&mut buf)
                .await
                .map_err(|e| e.to_string())?;
            if buf.len() > MAX_IMAGE_SIZE {
                return Err("Image exceeds 10MB limit".to_string());
            }
            Ok(buf)
        }
        None => Err("No body in image response".to_string()),
    }
}

/// Handle a binary (non-text) response by saving to disk.
async fn handle_binary_download(
    response: gemini_core::Response,
    url: &str,
    _meta: &str,
) -> Action<AppMessage> {
    use tokio::io::AsyncReadExt;

    let filename = url_to_filename(url);
    let download_dir = gemini_core::store::download_dir();

    if let Err(e) = std::fs::create_dir_all(&download_dir) {
        return Action::App(AppMessage::DownloadFailed(format!(
            "Cannot create download directory: {}", e
        )));
    }

    let path = unique_path(&download_dir, &filename);

    match response.into_body() {
        Some(mut body) => {
            let mut buf = Vec::new();
            match body.read_to_end(&mut buf).await {
                Ok(_) => {
                    let path_str = path.to_string_lossy().to_string();
                    match std::fs::write(&path, &buf) {
                        Ok(()) => Action::App(AppMessage::DownloadComplete {
                            filename,
                            path: path_str,
                        }),
                        Err(e) => Action::App(AppMessage::DownloadFailed(
                            format!("Write failed: {}", e),
                        )),
                    }
                }
                Err(e) => Action::App(AppMessage::DownloadFailed(
                    format!("Read failed: {}", e),
                )),
            }
        }
        None => Action::App(AppMessage::DownloadFailed(
            "No body in response".to_string(),
        )),
    }
}

/// Extract a filename from a URL path.
fn url_to_filename(url_str: &str) -> String {
    if let Ok(url) = Url::parse(url_str) {
        let path = url.path();
        if let Some(name) = path.rsplit('/').next() {
            if !name.is_empty() {
                return name.to_string();
            }
        }
        // Fallback to host
        return url.host_str().unwrap_or("download").to_string();
    }
    "download".to_string()
}

/// Generate a unique file path to avoid overwriting existing files.
fn unique_path(dir: &std::path::Path, filename: &str) -> std::path::PathBuf {
    let base = dir.join(filename);
    if !base.exists() {
        return base;
    }

    let stem = std::path::Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download");
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    for i in 1..1000 {
        let name = if ext.is_empty() {
            format!("{}_{}", stem, i)
        } else {
            format!("{}_{}.{}", stem, i, ext)
        };
        let path = dir.join(&name);
        if !path.exists() {
            return path;
        }
    }
    dir.join(format!("{}_{}", filename, std::process::id()))
}
