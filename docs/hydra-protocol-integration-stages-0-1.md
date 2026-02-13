# HYDRA Protocol Integration — Stages 0-1

## Context

HYDRA (Hashgraph-based Yielding Decentralized Rendezvous Architecture) is a protocol for decentralized, anonymous TLS certificate verification. It passively captures TLS certificate observations during normal browsing and builds a consensus ledger so users can detect targeted MITM attacks, CA compromise, and BGP hijacking.

cosmic-gemini already performs TLS handshakes with TOFU validation during Gemini browsing. This PR integrates the HYDRA protocol at Stages 0 (Seed — single node) and 1 (Pair — bilateral sync between 2 nodes), providing:

- **Stage 0**: Passive certificate observation capture, local diary, personal cert change detection
- **Stage 1**: Peer discovery via bootstrap, bilateral event sync, shared 2-observer certificate ledger

Full spec: `HYDRA-protocol-spec-v3.md`

---

## Architecture

### New crate: `hydra-core`

Separate workspace member crate. HYDRA has its own identity system (Ed25519 keypairs, not X.509 client certs), its own storage, its own networking, and its own crypto operations. Integration surface with `gemini-core` is narrow (one broadcast channel for cert captures).

### Transport: Arti (Rust Tor) from the start

Use `arti-client` (v0.39.0) with `onion-service-client` and `onion-service-service` features for real Tor transport from day one. All peer connections go through Tor .onion addresses.

- **Client connections**: `TorClient::create_bootstrapped()` → `tor_client.connect("peer.onion:port")` returns `DataStream` (impl `AsyncRead` + `AsyncWrite`)
- **Hosting .onion service**: `tor_client.launch_onion_service(config)` returns a `RunningOnionService` + stream of incoming `RendRequest`s
- **Bootstrap .onion**: Derived from the HKDF network identity key via `launch_onion_service_with_hsid()`
- **Node's personal .onion**: Derived from the node's Ed25519 keypair

A `Transport` trait still abstracts the connection layer so unit tests can use in-memory channels. But the production path is always Tor.

Arti caveat: *"not yet as secure as C-Tor"* — acceptable for a v0.3.0-draft protocol at Stages 0-1.

### Cert capture: broadcast channel on `Client`

Add an optional `broadcast::Sender<TlsCertCapture>` to `gemini_core::Client`. After TOFU validation in `fetch_internal_with_config()` (line 373), send the observation. Zero-cost when no subscriber.

### Event storage: JSONL append-only + JSON snapshots

- `events.jsonl` — append-only signed event log
- `observations.jsonl` — local observation diary
- `ledger.json` — computed certificate ledger (atomic rename writes)
- `peers.json` — known peer list

### Background node: tokio task with channels

`HydraNode` runs as a long-lived tokio task spawned in `App::init()`. Commands flow in via `mpsc::Sender<HydraCommand>`. Notifications flow out via `broadcast::Sender<HydraNotification>`. The UI bridges notifications into `AppMessage` variants via subscription.

---

## File Layout

### New: `hydra-core/`

```
hydra-core/
  Cargo.toml
  src/
    lib.rs                  # Public API re-exports
    error.rs                # HydraError enum (thiserror)
    crypto.rs               # Ed25519 sign/verify, HKDF-SHA256, SHA-256 helpers
    identity.rs             # NodeIdentity: generate/load Ed25519 keypair
    network_identity.rs     # HKDF network constant, bootstrap onion derivation
    observation.rs          # CertObservation, RawCertCapture, ObservationDiary (JSONL)
    event.rs                # SignedEvent, EventPayload, sign/verify/hash chain
    event_log.rs            # EventLog: append-only JSONL persistence
    ledger.rs               # CertificateLedger, DomainEntry, consensus rules (Sec 5.5)
    alert.rs                # AlertCheck: compare observation vs diary/ledger (Sec 5.6)
    peer.rs                 # PeerInfo, PeerList (JSON persistence)
    config.rs               # HydraConfig: enabled, port, sync interval, peers
    store.rs                # hydra_data_dir(), ensure_dir(), atomic_write()
    node.rs                 # HydraNode event loop, HydraHandle, HydraCommand, HydraNotification
    sync.rs                 # Bilateral sync: event exchange, timestamp ordering, merge
    transport/
      mod.rs                # Transport + TransportListener traits
      tor.rs                # TorTransport: arti-client based .onion connections + hosting
    bootstrap/
      mod.rs                # Re-exports
      protocol.rs           # Wire protocol: GET_PEERS, ANNOUNCE, SYNC_REQUEST/RESPONSE
      server.rs             # BootstrapServer: listen, handle connections
      client.rs             # BootstrapClient: discover_peers, announce, request_sync
```

### Modified: `gemini-core/`

- **`gemini-core/Cargo.toml`** — add `sync` to tokio features
- **`gemini-core/src/client.rs`** — add `TlsCertCapture` struct, add optional observer to `Client`, send capture after TOFU validation

### Modified: `cosmic-gemini/`

- **`Cargo.toml`** — add `hydra-core` workspace member + dependency
- **`src/message.rs`** — add HYDRA `AppMessage` variants
- **`src/tab.rs`** — add `TabContent::HydraPanel` variant
- **`src/model.rs`** — add `hydra_status`, `hydra_alerts` fields
- **`src/config.rs`** — add `hydra_enabled` to `AppConfig`
- **`src/app.rs`** — spawn `HydraNode` in `init()`, add subscription, add header status icon, route `HydraPanel` view
- **`src/update.rs`** — handle HYDRA messages (status, alerts, toggle, peer management, sync)
- **`src/menu.rs`** — add "HYDRA Panel" menu item
- **`src/views/mod.rs`** — add `pub mod hydra_view;`
- **`src/views/hydra_view.rs`** — new: HYDRA panel UI (node info, peers, alerts, observations, controls)
- **`src/views/cert_warning_view.rs`** — enhance with HYDRA observation context

---

## Implementation Steps (ordered)

### Step 1: `hydra-core` crate scaffold + core types

Create crate, `Cargo.toml`, `lib.rs`, `error.rs`, `store.rs`, `config.rs`.

**`hydra-core/Cargo.toml` dependencies:**
```toml
# Async runtime
tokio = { version = "1", features = ["sync", "time", "net", "io-util", "rt"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Crypto
sha2 = "0.10"
hex = "0.4"
ed25519-dalek = { version = "2", features = ["rand_core"] }
rand = "0.8"
hkdf = "0.12"
x509-parser = "0.16"

# Tor (Arti)
arti-client = { version = "0.39", features = [
    "tokio", "onion-service-client", "onion-service-service",
] }
tor-hsservice = "0.39"
tor-hscrypto = "0.39"        # HsIdKeypair for .onion identity

# Utilities
chrono = { version = "0.4", features = ["serde"] }
thiserror = "2"
log = "0.4"
dirs = "5"
async-trait = "0.1"
futures = "0.3"              # Stream trait for RendRequest handling
```

### Step 2: Crypto + Identity + Network Identity

- **`crypto.rs`**: `generate_keypair()`, `sign()`, `verify()`, `derive_network_identity()`, `sha256_hex()`
- **`identity.rs`**: `NodeIdentity { signing_key, verifying_key, node_id }`, `load_or_generate(data_dir)`
  - Store `node_key` (PKCS8 DER base64) and `node_id` (hex pubkey) in `~/.local/share/cosmic-gemini/hydra/`
- **`network_identity.rs`**: HKDF-SHA256 with spec constants:
  - IKM: `b"HYDRA-cert-verify-v1"`
  - Salt: `b"HYDRA-network-identity"`
  - Info: `b"ed25519-network-key"`
  - Compute and store the network pubkey + theoretical bootstrap onion address

### Step 3: Observation + Event + EventLog

- **`observation.rs`**:
  - `RawCertCapture { host, port, certs_der, timestamp }` — bridging type from gemini-core
  - `CertObservation { domain, port, cert_fingerprint, chain_fingerprints, issuer, subject, san, not_before, not_after, observed_at, observer_id }` — extracted from raw capture using `x509-parser`
  - `ObservationDiary` — JSONL file: `append()`, `load_all()`, `get_for_domain()`, `latest_for_domain()`
- **`event.rs`**:
  - `EventPayload` enum: `Genesis`, `MemberJoin`, `CertObservationBatch(Vec<CertObservation>)`
  - `SignedEvent { sequence, payload, author, timestamp, signature, prev_hash }`
  - `sign_event()`, `verify_event()`, `event_hash()`
- **`event_log.rs`**:
  - `EventLog` — JSONL persistence: `new()`, `append()`, `load()`, `verify_chain()`, `events_since()`

### Step 4: Ledger + Alert

- **`ledger.rs`**:
  - `DomainEntry { domain, consensus_cert, first_seen, last_seen, observation_count, observer_count, history, anomalies }`
  - `CertificateLedger` — computed from events via `compute_from_events()`, persisted as `ledger.json`
  - Implements consensus rules from spec Section 5.5 (Cases 1-4)
- **`alert.rs`**:
  - `AlertType`: `Verified`, `Unverified`, `Uncertain`, `TargetedAttack`, `NovelDivergence`
  - `check_observation(ledger, observation) -> AlertResult` — implements Section 5.6 algorithm
  - Stage 0: compare against local diary only
  - Stage 1: compare against shared 2-observer ledger

### Step 5: Peer + Tor Transport + Bootstrap Protocol

- **`peer.rs`**: `PeerInfo { node_id, onion_address, first_seen, last_seen, last_sync }`, `PeerList` (JSON)
- **`transport/mod.rs`**: `Transport` trait (`connect(addr) -> Stream`, `listen() -> Listener`), `TransportListener` trait (`accept() -> Stream`)
- **`transport/tor.rs`**: `TorTransport` wrapping `arti-client`:
  - **Init**: `TorClient::create_bootstrapped(config)` — bootstrap Tor circuits on node startup
  - **Connect to peer**: `tor_client.connect("peer_onion:port")` → `DataStream`
  - **Host .onion service**:
    - Node's personal .onion: `tor_client.launch_onion_service(config)` → `RunningOnionService` + incoming `RendRequest` stream
    - Bootstrap .onion: `tor_client.launch_onion_service_with_hsid(config, network_hsid_keypair)` — all nodes serve this shared address using the well-known HKDF-derived key
  - **Process incoming connections**: `handle_rend_requests()` converts `RendRequest` → `StreamRequest` → `DataStream` for each client
  - Tor data directory: `~/.local/share/cosmic-gemini/hydra/tor/` (Arti state, cached consensus, etc.)
- **`bootstrap/protocol.rs`**: Message framing — newline-delimited JSON messages:
  - `BootstrapMsg::GetPeers`, `Peers(Vec<PeerInfo>)`, `Announce { node_id, onion_address }`, `AnnounceAck`
  - `SyncRequest { since_sequence }`, `SyncResponse { events: Vec<SignedEvent> }`
- **`bootstrap/server.rs`**: `BootstrapServer::start(tor_transport, node_state) -> JoinHandle` — accepts connections on the node's .onion + the network bootstrap .onion
- **`bootstrap/client.rs`**: `BootstrapClient` methods: `discover_peers(bootstrap_onion)`, `announce()`, `request_sync()`, `send_events()`

### Step 6: Sync + Node

- **`sync.rs`**:
  - `bilateral_sync(local_log, remote_events) -> MergedLog`
  - Timestamp-based ordering with deterministic tie-breaking: if timestamps equal, lower `sha256(event_signature)` wins
  - Validate all incoming events (signatures, hash chain continuity)
- **`node.rs`**:
  - `HydraNode` — background tokio task:
    - Init: load or generate identity, bootstrap Tor (`TorClient::create_bootstrapped()`), launch personal .onion service, launch network bootstrap .onion (shared key), create genesis event if first run, load event log / diary / ledger
    - Event loop: `tokio::select!` on command channel, observation channel, sync timer, incoming Tor connections (from both .onion services)
    - On observation: extract metadata, batch into diary, create `SignedEvent`, append to log, recompute ledger, check alerts
    - On sync timer (Stage 1): connect to each known peer's .onion, exchange events
    - On incoming connection: handle GET_PEERS/ANNOUNCE/SYNC protocol
  - `HydraCommand`: `Shutdown`, `GetStatus`, `AddPeer(address)`, `RemovePeer(node_id)`, `ManualSync`, `ToggleEnabled`
  - `HydraNotification`: `Started(node_id)`, `Alert(AlertResult)`, `ObservationRecorded(domain)`, `SyncComplete(peer_id, events_exchanged)`, `StatusUpdate(HydraStatus)`, `Error(String)`
  - `HydraHandle { cmd_tx, join_handle }`
  - `HydraStatus { node_id, enabled, stage, observation_count, event_count, peer_count, domain_count, last_sync, alert_count }`

### Step 7: Certificate capture hook in gemini-core

**`gemini-core/src/client.rs`** changes:

```rust
// New struct
pub struct TlsCertCapture {
    pub host: String,
    pub port: u16,
    pub certs_der: Vec<Vec<u8>>,
    pub timestamp: std::time::SystemTime,
}

// Modify Client
pub struct Client {
    tls_config: Arc<rustls::ClientConfig>,
    cert_observer: Option<tokio::sync::broadcast::Sender<TlsCertCapture>>,
}

// Add builder method
pub fn with_cert_observer(mut self, tx: broadcast::Sender<TlsCertCapture>) -> Self {
    self.cert_observer = Some(tx);
    self
}
```

In `fetch_internal_with_config()` (currently static — change to `&self` method), after the TOFU block at line 373:

```rust
if let Some(ref observer) = self.cert_observer {
    if let Some(certs) = server_conn.peer_certificates() {
        let capture = TlsCertCapture {
            host: host.clone(),
            port,
            certs_der: certs.iter().map(|c| c.as_ref().to_vec()).collect(),
            timestamp: std::time::SystemTime::now(),
        };
        let _ = observer.send(capture);
    }
}
```

**Note**: `fetch_internal_with_config` is currently a static method. It needs to become `&self` or take the observer as a parameter. The cleanest approach is to make it `&self` since `Client` already holds `tls_config`, and pass `tls_config` override for `fetch_with_identity`. This requires updating `fetch_with_redirect` and `fetch_with_identity` call sites.

**`gemini-core/Cargo.toml`**: add `"sync"` to tokio features.

### Step 8: Workspace + App wiring

**`Cargo.toml` (root)**:
```toml
[workspace]
members = ["gemini-core", "hydra-core"]
```
Add `hydra-core = { path = "hydra-core" }` to `[dependencies]`.

**`src/app.rs`**:
- Add `hydra_handle: Option<hydra_core::HydraHandle>` and `cert_observer_tx: Option<broadcast::Sender<TlsCertCapture>>` to `App`
- In `init()`: if config `hydra_enabled`, create broadcast channel, spawn `HydraNode::start()`, store handle
- In `subscription()`: add HYDRA notification subscription (poll receiver → `AppMessage`)
- In `header_end()`: add shield icon showing HYDRA status (grey=off, green=active, amber=alert)
- In `view()`: match `TabContent::HydraPanel`

**`src/message.rs`** — add variants:
```rust
// HYDRA
HydraStatusUpdate(hydra_core::HydraStatus),
HydraAlert(hydra_core::alert::AlertResult),
HydraObservationRecorded(String),
ShowHydraPanel,
HydraToggleEnabled,
HydraAddPeer(String),        // address
HydraRemovePeer(String),     // node_id
HydraManualSync,
HydraDismissAlert(usize),
HydraPeerAddressChanged(String),
```

**`src/tab.rs`** — add variant:
```rust
HydraPanel {
    status: hydra_core::HydraStatus,
    alerts: Vec<hydra_core::alert::AlertResult>,
    recent_observations: Vec<(String, u64)>,  // (domain, count)
    new_peer_address: String,
},
```

**`src/model.rs`** — add fields:
```rust
pub hydra_status: Option<hydra_core::HydraStatus>,
pub hydra_alerts: Vec<hydra_core::alert::AlertResult>,
```

**`src/config.rs`** — add to `AppConfig`:
```rust
pub hydra_enabled: bool,  // default: false
```

### Step 9: Update handler + HYDRA view

**`src/update.rs`** — add match arms for all HYDRA `AppMessage` variants. Key handlers:
- `HydraStatusUpdate` → store in model, trigger redraw
- `HydraAlert` → store in alerts, show notification badge
- `ShowHydraPanel` → populate `TabContent::HydraPanel` from model state
- `HydraToggleEnabled` → send command to node, save config
- `HydraAddPeer` → send `AddPeer` command to node
- `HydraManualSync` → send `ManualSync` command to node

**`src/views/hydra_view.rs`** — new file:
- Node identity section (node_id display, personal .onion address, network identity)
- Stage indicator (Stage 0: Seed / Stage 1: Pair)
- Tor status (bootstrapping, connected, circuit count)
- Stats: observation count, event count, domain coverage, peer count
- Alerts list with dismiss buttons
- Peers section: list peers with .onion address and last sync time, add peer (text input + button), remove peer
- Toggle on/off
- Manual sync button (Stage 1 only)

**`src/views/cert_warning_view.rs`** — add optional HYDRA context section:
- "HYDRA has observed this domain N times since <date>"
- "Previous certificate fingerprint: <hex>"
- "Certificate changed on: <date>"

**`src/menu.rs`** — add keyboard shortcut `Ctrl+H` → `ShowHydraPanel`

### Step 10: Thread cert observer into fetch calls

In `update.rs`, wherever `Client::new()` is called (in `spawn_fetch`, `spawn_fetch_with_identity`, `spawn_image_fetches`), pass the cert observer channel:

```rust
let client = gemini_core::Client::new().with_cert_observer(observer_tx.clone());
```

The observer channel sender is cloned from `App`'s stored sender. The `HydraNode` holds a receiver and processes incoming captures.

---

## Critical Files Summary

| File | Action | Purpose |
|------|--------|---------|
| `hydra-core/` (new crate) | Create | Entire HYDRA protocol implementation |
| `gemini-core/src/client.rs` | Modify | Add cert observer channel (lines 208-244, 327-381) |
| `gemini-core/Cargo.toml` | Modify | Add tokio `sync` feature |
| `Cargo.toml` (root) | Modify | Add workspace member + dependency |
| `src/app.rs` | Modify | Spawn HydraNode, subscription, header icon (lines 69-88, 259-264) |
| `src/message.rs` | Modify | Add ~10 HYDRA message variants |
| `src/tab.rs` | Modify | Add `HydraPanel` variant |
| `src/model.rs` | Modify | Add hydra_status, hydra_alerts fields |
| `src/config.rs` | Modify | Add hydra_enabled to AppConfig |
| `src/update.rs` | Modify | Add HYDRA message handlers |
| `src/menu.rs` | Modify | Add HYDRA Panel menu item + Ctrl+H |
| `src/views/mod.rs` | Modify | Add `pub mod hydra_view;` |
| `src/views/hydra_view.rs` | Create | HYDRA panel view |
| `src/views/cert_warning_view.rs` | Modify | Add HYDRA observation context |

## Existing Code to Reuse

- `gemini-core/src/known_hosts.rs:36` — SHA-256 fingerprinting pattern: `hex::encode(Sha256::digest(cert_der))`
- `gemini-core/src/identity.rs` — Key storage patterns (`data_dir()`, file-based PEM, JSON config)
- `gemini-core/src/session.rs:32-47` — Atomic write pattern (write .tmp, rename)
- `gemini-core/src/store.rs` — `data_dir()` for XDG data directory
- `src/update.rs:991-1048` — `spawn_fetch()` async pattern with `Task::future()`
- `src/app.rs:259-264` — Subscription timer pattern for periodic tasks

---

## Storage Layout

```
~/.local/share/cosmic-gemini/hydra/
  node_key.pem              # Ed25519 private key (PKCS8 DER, base64)
  node_id                   # Hex-encoded public key
  config.json               # HydraConfig
  events.jsonl              # Append-only signed event log
  observations.jsonl        # Local observation diary
  ledger.json               # Computed certificate ledger
  peers.json                # Known peer list
  tor/                      # Arti state directory
    cache/                  # Cached Tor consensus, descriptors
    keys/                   # Onion service keys managed by Arti
```

---

## Verification

```bash
# 1. Build
cd ~/Projects/cosmic-gemini
cargo build

# 2. Unit tests (hydra-core) — no Tor required, uses mock transport
cargo test -p hydra-core
# Tests: crypto round-trips, identity gen/load, HKDF determinism,
# observation diary CRUD, event sign/verify/hash chain, event log
# append/load/integrity, ledger computation + consensus rules,
# alert detection, sync merge ordering, bootstrap protocol serialization

# 3. Run the app — Stage 0 (Tor bootstraps automatically)
cargo run
# - Enable HYDRA via menu or Ctrl+H panel → toggle on
# - Wait for "Tor bootstrapped" status in HYDRA panel (~10-30s)
# - HYDRA panel shows: node_id, personal .onion address, Stage 0
# - Browse a Gemini site → observation count increments
# - Visit same site again → "Verified" (matches diary)

# 4. Stage 1 bilateral sync (two instances on same or different machines)
# Instance A: enable HYDRA, note personal .onion address from panel
# Instance B: enable HYDRA, add Instance A's .onion as peer
# Both browse different Gemini sites
# Trigger manual sync → sync happens over Tor
# Both instances' ledgers converge — observations from both visible
```

---

## Deferred (future PRs)

- Stages 2-3: all-to-all sync, hashgraph consensus activation
- Heartbeat events, reputation scoring, dormancy
- Observation batching timer (currently observations are immediate; spec calls for 30-min batches)
- Observation shuffling for enhanced privacy
- Checkpoint anchoring
- Rate limiting / admission control
- Tor circuit recycling (1-4 hour randomized rotation per spec Section 12.1)
