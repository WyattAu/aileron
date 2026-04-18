# Sync Protocol Design

> **Status:** Design document -- no implementation
> **Created:** 2026-04-18
> **Phase:** K.20--K.23 (Sync Architecture)
> **Tasks:** TASK-K20, TASK-K21, TASK-K22, TASK-K23

## Overview

This document specifies the cross-device sync protocol for Aileron. All browser state is
encrypted end-to-end before leaving the local device. The server acts as a dumb storage
relay and never sees plaintext data. The design targets self-hosted deployment as the
primary transport, with a third-party option for users who prefer managed infrastructure.

### Design Principles

1. **Local-first** -- every device has a fully functional local store; sync is additive
2. **Zero-knowledge server** -- the server stores only opaque ciphertext blobs
3. **Delta sync** -- only changes since the last successful sync are transmitted
4. **Conflict-free by construction** -- CRDTs where needed, LWW-tombstones otherwise
5. **Self-hostable by default** -- the reference transport requires nothing beyond a
   generic HTTPS server with PUT/GET/DELETE

---

## Section 1: Data Model (TASK-K20)

### 1.1 Sync Collections

| Collection | CRDT Strategy | Typical Size | Sync Priority |
|------------|--------------|--------------|---------------|
| `bookmarks` | LWW-Element-Set | 50--500 items, ~2 KB each | High |
| `history` | Append-only log with LWW deletes | 10k--100k items, ~0.3 KB each | Medium |
| `settings` | LWW register (single document) | 1 item, ~4 KB | High |
| `tabs` | LWW-Element-Set (ephemeral) | 5--50 items, ~0.5 KB each | Low |
| `passwords` | LWW-Element-Set (references only) | 100--1000 items, ~0.4 KB each | High |
| `themes` | LWW-Element-Set | 1--10 items, ~8 KB each | Low |
| `content_scripts` | LWW-Element-Set | 5--20 items, ~3 KB each | Medium |

### 1.2 Item Schema

Every item in every collection conforms to the following envelope:

```json
{
  "id": "sha256:abcdef0123456789...",
  "collection": "bookmarks",
  "version": 1,
  "schema_version": 1,
  "created_at": "2026-04-18T12:00:00Z",
  "modified_at": "2026-04-18T14:30:00Z",
  "deleted": false,
  "tombstoned_at": null,
  "device_id": "dev_k8x2m9...",
  "vector_clock": {"dev_k8x2m9": 3, "dev_n4q1p7": 1},
  "payload": {
    "url": "https://example.com",
    "title": "Example",
    "parent_id": null,
    "position": 0
  }
}
```

Field semantics:

- `id` -- deterministic SHA-256 of `collection || stable_key` where `stable_key` is a
  collection-specific natural key (URL for bookmarks/history, path for settings, etc.)
- `version` -- monotonically increasing per-item version on the originating device
- `schema_version` -- global schema version; used for migration (see 1.4)
- `deleted` / `tombstoned_at` -- soft-delete via tombstone; tombstones are garbage-collected
  after 30 days
- `device_id` -- the device that last modified this item
- `vector_clock` -- per-device version counter for CRDT merge

### 1.3 Conflict Resolution

**LWW-Element-Set** is used for bookmarks, tabs, passwords, themes, and content_scripts.
Merge rule: highest `modified_at` wins. If timestamps are identical (within 1ms), the
lexicographically larger `device_id` wins. This is deterministic and requires no server
coordination.

**Append-only log** is used for history. Each history entry is immutable once written.
Deletes are recorded as tombstone entries appended to the log. This avoids any conflict
on history items -- they are never edited, only appended or tombstoned.

**LWW register** is used for settings. The settings collection is a single item whose
payload is the full settings document. Last-write wins on `modified_at`.

### 1.4 Schema Versioning

The `schema_version` field is a single integer incremented when the payload structure
changes. All devices must negotiate the minimum common schema version before syncing.

Migration rules:

1. On first sync after an upgrade, a device publishes its new `schema_version` in its
   device manifest (see 4.2).
2. If the server has items at a higher `schema_version` than the client supports, the
   client defers sync of those collections until it is upgraded.
3. If the client is at a higher `schema_version`, it migrates received items locally
   using a versioned migration function.
4. Backward migrations are not supported; the spec maintains a forward-only migration
   chain.

```
Schema version history:
  1  -- Initial release
  2  -- bookmarks: add favicon_url, tags fields
  3  -- tabs: add workspace_id, split_ratio fields
  ...
```

### 1.5 Delta Sync

Each device maintains a `sync_cursor` per collection -- a monotonically increasing
integer representing the last acknowledged server sequence number. On sync, the client
sends:

```json
{
  "collection": "bookmarks",
  "since_cursor": 142,
  "max_items": 500
}
```

The server responds with all items modified after cursor 142, up to `max_items`. If
the response is truncated (exactly `max_items` returned), the client paginates with
the new cursor from the last item in the batch.

### 1.6 Bandwidth Budget

Typical incremental sync payload (no full re-sync):

| Scenario | Upload | Download | Notes |
|----------|--------|----------|-------|
| Idle (no changes) | 120 B | 200 B | Cursor check, empty response |
| 5 bookmarks added | ~5 KB | ~3 KB | 5 items up, 0 down (or vice versa) |
| 1 hour browsing | ~2 KB | ~15 KB | History entries from other device |
| Full initial sync | ~800 KB | ~5 MB | All collections, compressed |
| Settings change | ~4 KB | ~4 KB | Single LWW register |

All payloads are compressed with zstd (level 3) before encryption. Expected compression
ratio on JSON payloads is 4:1--8:1.

---

## Section 2: Transport Evaluation (TASK-K21)

### 2.1 Candidate Transports

#### WebDAV

Store encrypted blobs as individual files in a WebDAV directory hierarchy:
`/{device_id}/collections/{collection}/{item_id}.json.enc`.

| Criterion | Rating | Notes |
|-----------|--------|-------|
| Self-hosting ease | High | Apache, nginx, radicale all support WebDAV out of the box |
| E2EE support | High | Client-side encryption before PUT; server never sees plaintext |
| Bandwidth efficiency | Medium | Per-file PUT/GET; no native delta support (mitigated by client-side cursors) |
| Latency | Medium | HTTP round-trips; no push (polling or long-poll required) |
| Implementation complexity | Low | HTTP client library; well-documented protocol |
| Reliability | High | Mature protocol; wide server support; idempotent operations |

#### Git-based

Each client commits encrypted blobs to a shared bare Git repository.

| Criterion | Rating | Notes |
|-----------|--------|-------|
| Self-hosting ease | High | Any Git server (gitea, cgit, bare repo over SSH) |
| E2EE support | High | Encrypt before commit |
| Bandwidth efficiency | Low | Full pack files on push/pull; pack negotiation adds overhead |
| Latency | Low | Push is fast; pull requires fetch + merge |
| Implementation complexity | Medium | libgit2 integration or shelling out to `git`; merge conflict handling |
| Reliability | High | Git's data integrity guarantees; reflog for recovery |

#### Custom Protocol over HTTPS

Purpose-built sync server with delta-aware endpoints.

| Criterion | Rating | Notes |
|-----------|--------|-------|
| Self-hosting ease | Low | Must deploy and maintain a custom server binary |
| E2EE support | High | Designed in from the start |
| Bandwidth efficiency | High | Native delta sync, cursor-based, binary protocol option |
| Latency | Low | WebSocket for push notifications; minimal round-trips |
| Implementation complexity | High | Full server implementation, client protocol, deployment story |
| Reliability | Medium | Depends on implementation quality; no existing ecosystem |

#### Matrix/Element

Use a Matrix room as the sync transport; each item is a Matrix event.

| Criterion | Rating | Notes |
|-----------|--------|-------|
| Self-hosting ease | Medium | Synapse/Dendrite deployment; significant resource requirements |
| E2EE support | High | Megolm built-in |
| Bandwidth efficiency | Low | Event overhead per item; full state sync on reconnect |
| Latency | Medium | Federation adds hops; /sync long-poll is decent |
| Implementation complexity | Medium | Matrix client library; room state management |
| Reliability | Medium | Dependent on Matrix server health; state resolution can be complex |

#### SQLite over SSH (litestream-style)

Stream encrypted SQLite WAL frames to a remote server via SSH.

| Criterion | Rating | Notes |
|-----------|--------|-------|
| Self-hosting ease | High | SSH server + disk space |
| E2EE support | Medium | SSH transport encryption; must encrypt SQLite file itself for E2EE |
| Bandwidth efficiency | High | WAL frame-level deltas |
| Latency | Low | Direct SSH tunnel |
| Implementation complexity | Medium | litestream or custom WAL replication |
| Reliability | Medium | SSH key management; single-file corruption risk |

### 2.2 Decision Matrix

| Criterion (weight) | WebDAV | Git | Custom HTTPS | Matrix | SQLite/SSH |
|--------------------|--------|-----|--------------|--------|------------|
| Self-hosting (0.25) | 5 | 4 | 2 | 3 | 5 |
| E2EE (0.25) | 5 | 5 | 5 | 5 | 3 |
| Bandwidth (0.15) | 3 | 2 | 5 | 2 | 5 |
| Latency (0.10) | 3 | 4 | 5 | 3 | 5 |
| Impl. complexity (0.15) | 5 | 3 | 1 | 3 | 3 |
| Reliability (0.10) | 5 | 5 | 3 | 3 | 3 |
| **Weighted total** | **4.55** | **3.80** | **3.30** | **3.15** | **4.00** |

### 2.3 Recommendation

**WebDAV is the primary transport.** Rationale:

1. WebDAV is available on virtually every self-hosted platform (Apache, nginx,
   Nextcloud, radicale, Caddy) with zero custom server deployment.
2. Client-side E2EE is fully under our control -- the server stores opaque blobs.
3. The protocol is simple (PUT/GET/DELETE/PROPFIND) and battle-tested.
4. Bandwidth inefficiency is acceptable: zstd compression and cursor-based delta sync
   at the application layer mitigate the lack of server-side delta support.
5. Implementation complexity is lowest, which matters for a 2-person project on a
   2-year timeline.

**Git-based transport is the secondary option**, recommended for users who already
run Git infrastructure and want the additional integrity guarantees. The two transports
share the same encrypted blob format; only the upload/download layer differs.

**Custom HTTPS** is deferred to a post-MVP phase. If the WebDAV transport proves
insufficient at scale, a custom server can be introduced without changing the
encryption or data model layers.

---

## Section 3: End-to-End Encryption (TASK-K22)

### 3.1 Cryptographic Primitives

| Purpose | Primitive | Notes |
|---------|-----------|-------|
| Key derivation | Argon2id (m=64MB, t=3, p=2) | From user passphrase + per-user salt |
| Data encryption | XChaCha20-Poly1305 ( libsodium `crypto_aead_xchacha20poly1305_ietf_encrypt`) | 192-bit nonce; 256-bit key |
| Item signing | Ed25519 (libsodium `crypto_sign_ed25519`) | Tamper detection; non-repudiation |
| Key wrapping | XChaCha20-Poly1305 | Wrap per-collection keys with master key |
| Key exchange | X25519 (libsodium `crypto_kx`) | For device-to-device key sharing |
| Hashing | BLAKE3 | Item IDs, integrity checks |
| Random | libsodium `randombytes_buf` | System CSPRNG |

### 3.2 Key Hierarchy

```
User Passphrase
    |
    v  Argon2id(passphrase, user_salt)
    |
Master Key (MK) -- 256 bits
    |
    +-- Ed25519 Signing Key Pair (SK_sign, PK_sign)
    |       |
    |       +-- used to sign all sync items (tamper detection)
    |
    +-- Per-Collection Encryption Keys (CEK)
    |       |
    |       +-- bookmarks_cek  = XChaCha20(MK, "aileron:cek:bookmarks")
    |       +-- history_cek    = XChaCha20(MK, "aileron:cek:history")
    |       +-- settings_cek   = XChaCha20(MK, "aileron:cek:settings")
    |       +-- tabs_cek       = XChaCha20(MK, "aileron:cek:tabs")
    |       +-- passwords_cek  = XChaCha20(MK, "aileron:cek:passwords")
    |       +-- themes_cek     = XChaCha20(MK, "aileron:cek:themes")
    |       +-- scripts_cek    = XChaCha20(MK, "aileron:cek:scripts")
    |
    +-- Key Wrapping Key (KWK) -- 256 bits
            |
            +-- used to encrypt CEKs for device-to-device sharing
```

Per-collection keys are derived from the master key via HKDF-BLAKE3 to enable
independent rotation without re-encrypting all collections.

### 3.3 Encrypted Item Format

Each item stored on the server is a binary blob:

```
[1 byte  : version = 0x01]
[32 bytes: nonce (random)]
[64 bytes: Ed25519 signature over ciphertext]
[variable: XChaCha20-Poly1305 ciphertext]
[16 bytes: Poly1305 MAC (included by AEAD)]
```

The plaintext fed to the AEAD is the JSON item (Section 1.2) serialized with
deterministic key ordering.

### 3.4 Device Registration

When a new device joins the sync group:

1. Device generates a local Ed25519 key pair `(SK_device, PK_device)`.
2. Device sends a **device registration request** to the server, signed by the user's
   master signing key:
   ```json
   {
     "type": "device_register",
     "device_id": "dev_k8x2m9...",
     "device_public_key": "base64...",
     "device_name": "workstation",
     "created_at": "2026-04-18T12:00:00Z",
     "signature": "base64..."
   }
   ```
3. The registration blob is stored on the server at
   `/{user_id}/devices/{device_id}.json.enc`.
4. Existing devices, on next sync, see the new device in the device manifest.
5. To share the master key with the new device, the originating device performs an
   **out-of-band key exchange** (see 3.5).

### 3.5 Key Sharing Between Devices

Direct passphrase entry on each device is the primary mechanism. For convenience,
an authorized key exchange protocol is provided:

1. Device A (already provisioned) generates an ephemeral X25519 key pair.
2. Device A encrypts the master key with Device B's public key:
   ```
   wrapped_mk = XChaCha20-Poly1305(
       key = HKDF-BLAKE3(shared_secret, "aileron:key_share"),
       plaintext = MK || MK_salt || SK_sign
   )
   ```
   where `shared_secret = X25519(SK_ephemeral_A, PK_device_B)`.
3. Device A uploads the wrapped key to `/{user_id}/key_shares/{device_id}.enc`.
4. Device B downloads and decrypts using its device private key.
5. Device B verifies the signing key by checking a signature on a known item.
6. Device B deletes the key share from the server after successful decryption.

This flow requires Device A to be online at the time of provisioning. If it is not,
the user enters the passphrase on Device B directly.

### 3.6 Key Rotation

**Collection key rotation:** To rotate a single collection's CEK (e.g., after a
suspected compromise):

1. Generate a new CEK via HKDF with a rotation counter: `CEK' = HKDF(MK, "aileron:cek:bookmarks:v2")`.
2. Re-encrypt all items in the collection with CEK'.
3. Upload re-encrypted items; old versions are overwritten.
4. Increment the rotation counter in the user manifest.

**Full master key rotation:** Change the passphrase and re-derive MK. All CEKs
change. All items must be re-encrypted and re-uploaded. This is an expensive
operation and should be rare.

### 3.7 Recovery

When all devices are lost, recovery is via a **recovery phrase** (BIP-39 mnemonic,
12 words) generated during initial setup:

1. During first-time setup, the user is shown a 12-word recovery phrase derived from:
   ```
   recovery_seed = Argon2id(passphrase, "aileron:recovery", m=256MB, t=1, p=1)
   mnemonic = BIP-39.encode(recovery_seed[:16 bytes])
   ```
2. The recovery phrase is written to paper by the user. It is never stored digitally.
3. To recover, the user enters the recovery phrase on a new device, which reconstructs
   the master key and all derived keys.
4. The device downloads all encrypted items from the server and decrypts them.
5. The user should change the passphrase after recovery (full master key rotation).

**Critical constraint:** The recovery phrase + passphrase together reconstruct the
master key. Losing both means data loss. The recovery phrase alone is useless without
the passphrase. This provides defense-in-depth: an attacker who finds the paper backup
still needs the passphrase.

---

## Section 4: Protocol Specification (TASK-K23)

### 4.1 Sync Flow Overview

```mermaid
sequenceDiagram
    participant D1 as Device A
    participant S as Server (WebDAV)
    participant D2 as Device B

    D1->>S: PUT /{user}/manifest.json.enc
    S-->>D1: 201 Created

    Note over D2: 30 minutes later

    D2->>S: GET /{user}/manifest.json.enc
    S-->>D2: 200 OK (manifest)

    D2->>S: GET /{user}/collections/bookmarks?since=0
    S-->>D2: 200 OK (encrypted items batch)

    D2->>D2: Decrypt, merge, apply
    D2->>S: PUT /{user}/manifest.json.enc (updated cursor)
    S-->>D2: 201 Created
```

### 4.2 Server-Side File Layout

```
/{user_id}/
  manifest.json.enc              -- user manifest (device list, cursors, schema version)
  devices/
    {device_id}.json.enc         -- device registration blob
  key_shares/
    {device_id}.enc              -- wrapped master key for pending devices
  collections/
    {collection}/
      {item_id}.json.enc         -- encrypted sync item
      _cursor                    -- server-side cursor tracker (plaintext integer)
      _meta.json                 -- collection metadata (item count, latest cursor)
```

The `_cursor` and `_meta.json` files are plaintext because they contain no sensitive
information -- only sequence numbers and counts.

### 4.3 User Manifest

```json
{
  "user_id": "usr_a1b2c3...",
  "schema_version": 3,
  "devices": {
    "dev_k8x2m9": {
      "name": "workstation",
      "public_key": "base64...",
      "last_seen": "2026-04-18T14:30:00Z",
      "client_version": "0.3.0"
    },
    "dev_n4q1p7": {
      "name": "laptop",
      "public_key": "base64...",
      "last_seen": "2026-04-18T12:00:00Z",
      "client_version": "0.3.0"
    }
  },
  "cursors": {
    "bookmarks": 142,
    "history": 8931,
    "settings": 7,
    "tabs": 54,
    "passwords": 203,
    "themes": 2,
    "content_scripts": 11
  },
  "collection_key_versions": {
    "bookmarks": 1,
    "history": 1,
    "settings": 1,
    "tabs": 1,
    "passwords": 1,
    "themes": 1,
    "content_scripts": 1
  }
}
```

The manifest is encrypted with the master key and signed with the master signing key.
Only one device writes the manifest at a time; conflicts are resolved by the LWW rule
on `modified_at`.

### 4.4 API Endpoints

All endpoints use HTTPS. Authentication is via a bearer token derived from the master
key (see 4.6).

#### Collection Operations

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/{user}/manifest.json.enc` | Fetch user manifest |
| `PUT` | `/{user}/manifest.json.enc` | Upload updated manifest |
| `GET` | `/{user}/collections/{collection}/?since={cursor}&limit={n}` | Fetch items since cursor |
| `PUT` | `/{user}/collections/{collection}/{item_id}.json.enc` | Upload/create item |
| `DELETE` | `/{user}/collections/{collection}/{item_id}.json.enc` | Tombstone an item (client sets `deleted: true` before upload) |
| `GET` | `/{user}/collections/{collection}/_meta.json` | Collection metadata (item count, latest cursor) |
| `PUT` | `/{user}/devices/{device_id}.json.enc` | Register device |
| `GET` | `/{user}/devices/{device_id}.json.enc` | Fetch device registration |
| `PUT` | `/{user}/key_shares/{device_id}.enc` | Upload wrapped key share |
| `GET` | `/{user}/key_shares/{device_id}.enc` | Download wrapped key share |
| `DELETE` | `/{user}/key_shares/{device_id}.enc` | Consume key share (delete after use) |

#### Delta Sync Request

```
GET /{user}/collections/bookmarks/?since=142&limit=500 HTTP/1.1
Host: sync.example.com
Authorization: Bearer ail_usr_a1b2c3_t7k9m2...
```

#### Delta Sync Response

```
HTTP/1.1 200 OK
Content-Type: application/octet-stream
Content-Encoding: zstd
X-Aileron-Cursor: 147
X-Aileron-Truncated: false

<binary: concatenated encrypted item blobs>
```

Each blob in the response body is prefixed with a 4-byte little-endian length.
The client reads blobs until the body is exhausted.

### 4.5 Authentication

The sync server authenticates clients via a bearer token:

```
token = BLAKE3(MK || "aileron:auth_token")[..32 bytes]
```

This token is derived from the master key and is unique per user. The server stores
a mapping of `token_hash -> user_id` (using BLAKE3 hash of the token to prevent
token exposure in server memory).

**WebDAV-specific note:** WebDAV servers typically use Basic auth or Bearer tokens.
For self-hosted setups, the user configures the token as the WebDAV password. For the
custom HTTPS transport, the token is sent as a standard `Authorization: Bearer` header.

### 4.6 Conflict Resolution Protocol

```mermaid
flowchart TD
    A[Client receives items from server] --> B{For each item: compare vector_clock with local}
    B -->|Server is ahead| C[Apply server version]
    B -->|Local is ahead| D[Keep local version, queue for upload]
    B -->|Diverged| E{Collection type?}
    E -->|LWW| F[Compare modified_at; keep winner]
    E -->|Append-only log| G[Keep both; append local entry]
    F --> H[Upload merged state]
    D --> H
    C --> I[Update local cursor]
    H --> I
```

For LWW collections, the merge is deterministic: both clients independently arrive at
the same result. If Device A has item V1 at time T1 and Device B has item V2 at time T2,
the item with the later `modified_at` wins. The losing version is discarded.

### 4.7 Error Handling

#### Offline Mode

When the server is unreachable, the client continues operating locally. All changes
are accumulated in a local write-ahead log (WAL). On reconnection:

1. Client fetches the current server manifest.
2. Client compares its local WAL against the server's cursor.
3. Client performs a merge as described in 4.6.
4. Client uploads all pending changes.
5. If the WAL grows beyond 100 MB, the client warns the user that a long offline
   period may cause sync issues.

#### Server Errors

| HTTP Status | Meaning | Client Action |
|-------------|---------|---------------|
| `200` | Success | Process response |
| `201` | Created | Confirm write |
| `401` | Unauthorized | Re-derive auth token; prompt for passphrase if MK unavailable |
| `403` | Forbidden | Token valid but access denied; check server configuration |
| `404` | Not Found | Item does not exist; treat as empty for GET, create for PUT |
| `409` | Conflict | WebDAV lock conflict; retry with exponential backoff (max 5 attempts) |
| `429` | Too Many Requests | Back off per `Retry-After` header |
| `500` | Server Error | Retry with exponential backoff (base 2s, max 60s, jitter 0.5s) |
| `503` | Unavailable | Queue for later; enter offline mode |

#### Data Integrity

Every decrypted item is verified against its Ed25519 signature before being applied.
If signature verification fails, the item is discarded and an error is logged. This
detects server-side tampering or data corruption.

### 4.8 Example Sync Flows

#### Initial Sync (New Device)

```
1. User enters passphrase on Device B.
2. Device B derives MK via Argon2id.
3. Device B fetches manifest from server.
4. Manifest decryption fails (no wrapped key share available; Device A is offline).
5. User enters recovery phrase.
6. Device B reconstructs MK from passphrase + recovery phrase.
7. Device B decrypts manifest; obtains cursors and device list.
8. Device B fetches all items from each collection (since cursor 0).
9. Device B decrypts and applies all items to local store.
10. Device B registers itself in the manifest.
11. Device B uploads updated manifest.
12. Sync complete. Device B is now fully operational.
```

#### Incremental Sync (Normal Operation)

```
1. Device A adds 3 bookmarks locally.
2. Device A encrypts the 3 new bookmark items.
3. Device A uploads the 3 items to the server.
4. Device A fetches the manifest, increments bookmark cursor to N+3.
5. Device A uploads updated manifest.
6. [Later] Device B initiates sync.
7. Device B fetches manifest; sees bookmark cursor advanced by 3.
8. Device B fetches items since its last bookmark cursor.
9. Device B receives 3 encrypted items.
10. Device B decrypts and verifies signatures.
11. Device B merges into local store (no conflicts; items are new).
12. Device B updates its local cursor.
13. Device B uploads updated manifest (to record its last-seen cursors).
14. Sync complete.
```

#### Conflict Scenario (Concurrent Edits)

```
1. Device A and Device B both have bookmark "GitHub" with cursor=100.
2. Device A changes title to "GitHub - Code" at T1.
3. Device B changes title to "GitHub Code Hosting" at T2, where T2 > T1.
4. Both devices sync:
   a. Device A uploads its version (title="GitHub - Code").
   b. Device B fetches server state, sees Device A's version.
   c. Device B compares vector_clocks: both have the same parent, diverged.
   d. Device B compares modified_at: T2 > T1, so Device B's version wins.
   e. Device B uploads its version (title="GitHub Code Hosting").
5. Device A syncs again:
   a. Device A fetches server state, sees Device B's version.
   b. Device A compares modified_at: T2 > T1, server wins.
   c. Device A applies the server version locally.
6. Final state: "GitHub Code Hosting" on both devices.
```

#### Password Reference Sync

```
1. Device A saves a password for "github.com" in the local keyring.
2. Device A creates a password reference item:
   {
     "id": "sha256:...",
     "collection": "passwords",
     "payload": {
       "domain": "github.com",
       "username": "user@example.com",
       "credential_id": "local_keyring_ref_abc123",
       "created_at": "2026-04-18T12:00:00Z"
     }
   }
3. Device A encrypts and uploads the reference.
4. Device B syncs, receives the reference.
5. Device B sees "github.com" / "user@example.com" in its password list.
6. When Device B navigates to github.com, it checks its local keyring for
   a matching entry. If not found, it prompts the user to enter the password
   and stores it in Device B's local keyring with the same credential_id.
7. The actual password value never traverses the network.
```

### 4.9 Sync Schedule

| Event | Trigger | Action |
|-------|---------|--------|
| Startup sync | Application launch | Full manifest check + delta sync all collections |
| Periodic sync | Every 5 minutes (configurable) | Delta sync all collections |
| Event-driven sync | Local write (bookmark, setting, etc.) | Debounced 2s, then delta sync the affected collection |
| User-initiated sync | Manual keystroke (e.g., `gs` in Normal mode) | Full delta sync all collections |
| Shutdown sync | Application quit | Upload all pending changes; fail-fast if upload errors |

### 4.10 Tombstone Garbage Collection

Tombstones are retained for 30 days to ensure all devices see the deletion before
it is purged. Garbage collection runs weekly:

1. Fetch all items with `deleted: true` and `tombstoned_at < now - 30 days`.
2. Verify that all known devices have synced past the tombstone's cursor.
3. Delete the tombstone items from the server.
4. Compact the cursor space (optional: rewrite all remaining items with
   sequential IDs to eliminate gaps).
