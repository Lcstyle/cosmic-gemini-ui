# PR 12: Convergence-Enhanced TOFU Certificate Validation

## Context

PRs 7-11 (Feature Wave 2) are fully implemented. The browser currently uses pure TOFU for certificate validation: on first visit, the cert is pinned; on cert change, a `BadIdentity` error triggers `CertWarning` UI where the user must blindly "Trust New Certificate" or go back. This is vulnerable to MITM attacks during first visit and provides no second opinion on cert changes.

PR 12 adds **Convergence** — a decentralized certificate validation system inspired by Moxie Marlinspike's Convergence project. Multiple notary nodes independently observe server certificates and vote on what they see. When a cert changes, the browser queries notaries and uses quorum consensus to auto-accept legitimate changes or warn about conflicts.

**Design choice: Option B** — Gemini protocol as transport for notary queries. Notaries are themselves Gemini servers that respond on `/notary?target=<url>` and `/peers` endpoints.

---

## Sub-PR Dependency Map

```
Sub-PR 1 (Trust Engine Core)
    |
    v
Sub-PR 2 (Notary Client + UI)
    |
    +------------------+
    v                  v
Sub-PR 3 (Server)    Sub-PR 4 (Discovery + Gossip)
```

---

## Sub-PR 1: Trust Engine Core

### Goal
Add the convergence data model, config, Ed25519 crypto, observation cache, and trust decision engine. No behavioral change when disabled (default).

### New Dependencies (`gemini-core/Cargo.toml`)
```toml
base64 = "0.22"    # Encoding observation data and share codes
ring = "0.17"      # Ed25519 signing (already transitive via rustls, make explicit)
```

### New Module: `gemini-core/src/convergence/`

**`mod.rs`** — Module root:
```rust
pub mod types;
pub mod config;
pub mod crypto;
pub mod store;
pub mod trust_engine;
```

**`types.rs`** — Core data types:
- `NotaryPeer { id, address, spki_sha256, trusted, failure_count }` (Serialize/Deserialize)
- `PeerEntry { notary_id, address, spki_sha256 }` — peer gossip exchange format
- `Observation { notary_id, target_host, cert_sha256, spki_sha256, timestamp, signature }` — signed notary observation
- `ObservationResult { Verified | Unverified | Error(String) }` — sig verification result
- `TrustDecision` enum:
  - `Accept { quorum_count, total_queried }` — auto-pin
  - `Conflict { matching: Vec<Observation>, divergent: Vec<Observation> }` — show UI
  - `FallbackTofu` — no notaries reachable, fall back to existing TOFU prompt
  - `AlreadyTrusted` — cert matches known_hosts

**`config.rs`** — Stored at `~/.local/share/cosmic-gemini/convergence/config.json`:
```rust
NotaryConfig {
    enabled: bool,              // default: false (opt-in)
    strict_mode: bool,          // default: false (warn vs refuse on conflict)
    quorum_threshold: usize,    // default: 2
    query_timeout_ms: u64,      // default: 5000
    max_observation_age_secs: u64, // default: 86400
    notary_server_enabled: bool,   // default: false
    notary_server_port: u16,       // default: 11965
    peers: Vec<NotaryPeer>,
}
```
Functions: `load_config()`, `save_config()`, `convergence_dir()` — pattern from `identity.rs:40-64`.

**`crypto.rs`** — Ed25519 via `ring`:
- `load_or_generate_signing_key()` — stored at `convergence_dir()/server_key.pem`
- `sign(key, data) -> String` (base64)
- `verify(public_key_bytes, data, sig_base64) -> bool`
- `notary_id_from_key(key) -> String`

**`store.rs`** — Observation cache at `convergence_dir()/observations.json`:
- `ObservationStore { observations: Vec<Observation> }`
- `load()`, `save()`, `add()`, `get_for_host()`, `prune_stale(max_age)`

**`trust_engine.rs`** — Decision logic:
```rust
pub fn evaluate(
    config: &NotaryConfig,
    known_hosts: &impl KnownHostsRepo,
    observations: &[Observation],
    host: &str,
    observed_cert_sha256: &str,
) -> TrustDecision
```
Logic: (1) cert matches known_hosts -> AlreadyTrusted, (2) convergence disabled/no peers -> FallbackTofu, (3) count matching observations >= quorum -> Accept, (4) observations disagree -> Conflict, (5) no responses -> FallbackTofu.

### Changes to Existing Files

**`gemini-core/src/lib.rs`** — Add `pub mod convergence;`

**`gemini-core/src/known_hosts.rs`** — Add standalone functions:
- `cert_sha256(cert_der: &[u8]) -> String` — extract from existing `validate()` line 36
- `spki_sha256(cert_der: &[u8]) -> String` — extract SPKI field from X.509 DER, hash it (more stable across cert renewals)

**`gemini-core/src/client.rs`** — Add new error variant:
```rust
#[error("Certificate needs convergence verification")]
ConvergenceCheck {
    host: String,
    cert_sha256: String,
    spki_sha256: String,
    url: String,
},
```
Modify `fetch_internal_with_config()` lines 330-344: when `validate()` returns `BadIdentity` AND convergence is enabled (checked via `NotaryConfig::load_config()`), return `Error::ConvergenceCheck` instead of `Error::Tls(BadIdentity)`. When convergence is disabled, behavior is identical to current.

### Tests
- `trust_engine::evaluate()` with mocked observations (quorum, conflict, fallback)
- `ObservationStore` round-trip save/load
- `NotaryConfig` defaults + serialization
- `crypto::sign()` / `verify()` round-trip
- `known_hosts::spki_sha256()` on test certificates

---

## Sub-PR 2: Notary Client + Async Integration + Enhanced UI

### Goal
Query remote notaries over Gemini protocol, wire into `Task::future()` pattern, add convergence-aware trust decision UI.

### New Files

**`gemini-core/src/convergence/notary_client.rs`**:
- `query_notary(notary_address, target_url) -> Result<Observation>` — fetches `gemini://<addr>/notary?target=<url>`, parses notary-v1 response
- `query_peers(notary_address) -> Result<Vec<PeerEntry>>` — fetches `/peers`, parses peers-v1 response
- `query_all_notaries(config, target_url) -> Vec<Result<Observation>>` — fan-out with per-notary timeout using `tokio::time::timeout`

**`gemini-core/src/convergence/protocol.rs`** — Format constants + serializers:
- `NOTARY_PATH`, `PEERS_PATH`, `NOTARY_FORMAT_VERSION = "notary-v1"`, `PEERS_FORMAT_VERSION = "peers-v1"`
- `format_observation(obs) -> String` — gemtext preformatted block
- `format_peers(notary_id, peers) -> String`
- `parse_notary_response(body) -> Result<Observation>`
- `parse_peers_response(body) -> Result<Vec<PeerEntry>>`

**`src/views/convergence_view.rs`** — Three views:
1. `checking_view(url, host)` — "Verifying certificate with notaries..." spinner
2. `trust_decision_view(url, host, decision, observations)` — shows notary results, Accept/Reject/Query Again/Go Back buttons
3. `notary_manager_view(config)` — list trusted+candidate notaries, add/remove, toggle convergence, toggle server

### Changes to `src/tab.rs`
New `TabContent` variants:
```rust
ConvergenceChecking { url: String, host: String },
ConvergenceDecision {
    url: String, host: String,
    decision: TrustDecision,
    observations: Vec<Observation>,
    cert_sha256: String, spki_sha256: String,
},
NotaryManager { config: NotaryConfig },
```

### Changes to `src/message.rs`
New `AppMessage` variants:
```rust
ConvergenceCheck { url, host, cert_sha256, spki_sha256 },
ConvergenceResult { url, decision, observations, cert_sha256, spki_sha256 },
ConvergenceAccept { url, host, cert_sha256 },
ConvergenceReject { url },
ConvergenceQueryAgain { url, host, cert_sha256, spki_sha256 },
ShowNotaryManager,
AddNotaryPeer { address: String },
RemoveNotaryPeer { id: String },
PromoteNotaryPeer { id: String },
ToggleConvergence,
```

### Changes to `src/update.rs`
1. `spawn_fetch()` (line 885-895): When error is `ConvergenceCheck`, emit `AppMessage::ConvergenceCheck` instead of `CertWarning`
2. `ConvergenceCheck` handler: Set `TabContent::ConvergenceChecking`, spawn async task that queries all notaries -> `ConvergenceResult`
3. `ConvergenceResult` handler:
   - `Accept` -> auto-pin cert (remove old known_hosts entry), set Loading, re-fetch
   - `Conflict` -> set `TabContent::ConvergenceDecision`
   - `FallbackTofu` -> fall through to existing `CertWarning` view
4. `ConvergenceAccept` -> same as `TrustCertificate` (remove old entry, re-fetch)
5. Notary manager handlers

### Changes to `src/app.rs`
- View matching for new `TabContent` variants
- `Ctrl+N` -> `ShowNotaryManager`

### Changes to `src/views/mod.rs`
- Add `pub mod convergence_view;`

### Changes to `src/config.rs`
- Add `convergence_enabled: bool` and `convergence_strict: bool` to `AppConfig`

---

## Sub-PR 3: Notary Server

### Goal
Each browser node can serve notary queries on a local Gemini server. Responds to `/notary?target=<url>` and `/peers`; everything else returns `51 NOT FOUND`.

### New File: `gemini-core/src/convergence/notary_server.rs`

```rust
pub struct NotaryServerState {
    pub config: NotaryConfig,
    pub signing_key: Ed25519KeyPair,
    pub notary_id: String,
}

pub async fn start_server(config: NotaryConfig) -> Result<JoinHandle<()>, String>
```

Flow:
1. Load/generate Ed25519 key + self-signed TLS cert (via rcgen, already a dependency)
2. Bind `0.0.0.0:<config.notary_server_port>` with TLS acceptor
3. Route: `/notary?target=<url>` -> connect to target, observe cert, sign observation, return notary-v1 response. `/peers` -> return peers-v1 response. Other -> `51 NOT FOUND\r\n`

### Changes to Existing Files
- `src/app.rs` `init()`: if `notary_server_enabled`, spawn server
- `src/message.rs`: `NotaryServerStarted`, `NotaryServerFailed(String)`, `ToggleNotaryServer`
- `src/views/convergence_view.rs`: Server status indicator in notary manager

---

## Sub-PR 4: Discovery, Gossip, and Notary Link Detection

### Goal
Three discovery strategies: (1) detect notary links in browsed gemtext, (2) social import via share codes, (3) periodic gossip via `/peers` on trusted notaries.

### New File: `gemini-core/src/convergence/discovery.rs`
- `extract_notary_links(blocks: &[Block]) -> Vec<String>` — scan for `gemini://<host>:<port>/notary` patterns
- `generate_share_code(address, spki_sha256) -> String` — base64 encoding
- `parse_share_code(code) -> Result<NotaryPeer>`
- `gossip_round(config) -> Vec<NotaryPeer>` — query `/peers` on all trusted, merge candidates
- `evaluate_candidate_promotion(candidate, trusted_peers, peer_lists, threshold) -> bool`

### Changes
- `src/update.rs` `PageLoaded` handler: if convergence enabled, scan blocks for notary links, add as candidates
- `src/message.rs`: `ImportNotaryShareCode(String)`, `GossipRound`, `GossipRoundComplete(Vec<NotaryPeer>)`, `PromoteCandidateResult { id, promoted }`
- `src/views/convergence_view.rs`: Candidates section, share code import, "Share my notary" button, "Run gossip" button
- `src/app.rs` subscription: periodic gossip timer (every 30min when convergence enabled)

---

## Complete File Inventory

### New Files (11)
| File | Sub-PR |
|---|---|
| `gemini-core/src/convergence/mod.rs` | 1 |
| `gemini-core/src/convergence/types.rs` | 1 |
| `gemini-core/src/convergence/config.rs` | 1 |
| `gemini-core/src/convergence/crypto.rs` | 1 |
| `gemini-core/src/convergence/store.rs` | 1 |
| `gemini-core/src/convergence/trust_engine.rs` | 1 |
| `gemini-core/src/convergence/notary_client.rs` | 2 |
| `gemini-core/src/convergence/protocol.rs` | 2 |
| `gemini-core/src/convergence/notary_server.rs` | 3 |
| `gemini-core/src/convergence/discovery.rs` | 4 |
| `src/views/convergence_view.rs` | 2 |

### Modified Files
| File | Sub-PR | Changes |
|---|---|---|
| `gemini-core/Cargo.toml` | 1 | +base64, +ring (explicit) |
| `gemini-core/src/lib.rs` | 1 | +`pub mod convergence;` |
| `gemini-core/src/known_hosts.rs` | 1 | +`cert_sha256()`, +`spki_sha256()` standalone fns |
| `gemini-core/src/client.rs` | 1,2 | +`Error::ConvergenceCheck` variant; conditional flow in `fetch_internal_with_config()` lines 330-344 |
| `src/tab.rs` | 2 | +`ConvergenceChecking`, `ConvergenceDecision`, `NotaryManager` variants |
| `src/message.rs` | 2,3,4 | +convergence AppMessage variants |
| `src/update.rs` | 2,3,4 | +convergence handlers; modify `spawn_fetch()` error path (lines 885-895) |
| `src/app.rs` | 2,3,4 | +view matching, +Ctrl+N, +server startup, +gossip timer |
| `src/views/mod.rs` | 2 | +`pub mod convergence_view;` |
| `src/config.rs` | 2 | +`convergence_enabled`, +`convergence_strict` |

---

## Key Architectural Decisions

**Why `Error::ConvergenceCheck` instead of making `validate()` async?**
The existing `validate()` is synchronous inside `fetch_internal_with_config()`. Convergence queries are async (network I/O). Surfacing the need as a typed error variant lets the UI layer orchestrate the async flow via `Task::future()`, consistent with `spawn_fetch()`, `spawn_fetch_with_identity()`, `spawn_image_fetches()`.

**Why opt-in disabled by default?**
Zero behavioral change for users who don't enable it. The `BadIdentity` -> `CertWarning` path remains identical. Convergence only activates when `NotaryConfig.enabled == true`.

**Why Gemini as notary transport?**
Keeps the entire stack within the Gemini protocol ecosystem. Notary queries reuse `gemini_core::Client`, no HTTP dependencies needed. Responses use gemtext with preformatted blocks for structured data.

**Why store config at `data_dir()` not COSMIC config?**
Complex nested data (peer lists, observations, keys) doesn't fit COSMIC's simple key-value config. Follows the `identity.rs` pattern. Two boolean flags are mirrored in `AppConfig` for settings panel.

---

## Verification

```bash
# Sub-PR 1: Core engine
cargo test -p gemini-core  # trust_engine, store, crypto, config tests
# Existing TOFU still works when convergence disabled

# Sub-PR 2: Client + UI
cargo run  # Visit a host with changed cert -> convergence checking UI
           # With no peers configured -> FallbackTofu -> existing CertWarning
           # Ctrl+N -> notary manager

# Sub-PR 3: Server
cargo run  # Enable notary server -> binds port 11965
           # Query with another instance: gemini://localhost:11965/notary?target=gemini://example.com

# Sub-PR 4: Discovery
cargo run  # Browse capsule with notary links -> candidates appear in manager
           # Import share code -> new peer added
           # Gossip round discovers peers from trusted notaries
```
