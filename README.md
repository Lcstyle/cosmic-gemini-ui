# Cosmic Gemini

A Gemini protocol browser built with [libcosmic](https://github.com/pop-os/libcosmic) for the COSMIC desktop environment.

## Features

- **Tabbed browsing** with a COSMIC-style segmented tab bar, close buttons, middle-click to close, and right-click context menu
- **Gemtext rendering** with headings, paragraphs, links, preformatted blocks, quotes, and list items
- **Inline image loading** for linked images (PNG, JPEG, GIF, WebP) served over Gemini
- **Navigation** with back/forward history, reload, home page, and keyboard shortcuts
- **Bookmarks** — save and view bookmarked pages
- **Session persistence** — tabs and history are saved/restored across sessions
- **TOFU certificate validation** with certificate change warnings
- **Client identity management** — generate, import, and bind X.509 client certificates for authenticated Gemini capsules
- **Titan uploads** — compose and submit content to Titan-enabled servers
- **Misfin messaging** — send messages via the Misfin protocol
- **Binary downloads** — non-text responses are saved to disk with open-file support

## Architecture

The project is a Cargo workspace with two crates:

- **`cosmic-gemini`** — the COSMIC application (UI, views, message handling)
- **`gemini-core`** — protocol library (TLS client, Gemini/Titan/Misfin, parser, TOFU, identity store, session persistence)

## Requirements

- Rust 1.80+
- A running COSMIC desktop session (or Wayland compositor)
- System dependencies for libcosmic (see [libcosmic build instructions](https://github.com/pop-os/libcosmic#building))

## Building

```bash
cargo build --release
```

## Running

```bash
# Launch with home page or blank tab
cargo run --release

# Navigate directly to a URL
cargo run --release -- gemini://geminiprotocol.net
```

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+L` / `F6` | Focus URL bar |
| `Ctrl+T` | New tab |
| `Ctrl+W` | Close active tab |
| `Ctrl+R` / `F5` | Reload |
| `Ctrl+D` | Bookmark page |
| `Ctrl+I` | Identity manager |
| `Ctrl+Tab` | Next tab |
| `Ctrl+Shift+Tab` | Previous tab |
| `Alt+Left` | Back |
| `Alt+Right` | Forward |
| `Alt+Home` | Home page |

## License

This project is provided as-is. See individual dependency licenses for details.
