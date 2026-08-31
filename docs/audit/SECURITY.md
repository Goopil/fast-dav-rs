# Security Review — fast-dav-rs 0.9.0

**Audit date:** 2026-08-31 · Companion to `FINDINGS.md` (full Evidence/Trigger/Remediation blocks live there; this document is the security-only view).

**Thrust model assumed:** the DAV server is semi-trusted (it holds the user's data but its hrefs/property values must not become a credential-egress or memory-exhaustion vector); the network may be hostile unless TLS; embedding applications are trusted.

## 1. Vulnerabilities and risks

### SEC-1 — Credential egress to server-controlled origins (AUDIT-004) — **High**
- **Attacker capability:** malicious/compromised server, or any influence over href strings returned in multistatus.
- **Attack surface:** every client method that accepts a path/href (`get`, `propfind`, `delete_if_match`, `copy`/`move` Destination, sync flows feeding `SyncItem.href` back in).
- **Precondition:** client configured with credentials; server (or MITM when SEC-2 is on) returns an absolute URL on a different origin, or caller passes attacker-influenced strings.
- **Impact:** Basic/Bearer credentials transmitted cross-origin. Silent by construction; no redirects are followed, so this is the only egress path.
- **Mitigation:** origin check in `build_uri`/`send` (`src/webdav/client.rs:263-267, 559-561`); strip auth for cross-origin unless allow-listed.

### SEC-2 — TLS/cleartext footguns (AUDIT-011) — **Medium**
- `danger_accept_invalid_certs` → `NoVerify` verifier (`src/webdav/builder.rs:405-466`): disables cert **and hostname** verification; warning only in debug builds — silent in release.
- Plain `http://` accepted by default with credentials attached to every request (auth header built once at `builder.rs:372-396`).
- **Impact:** credentials in cleartext or exposed to MITM, invisibly in production builds.
- **Mitigation:** release-mode loud warning/failure for both; keep explicit escape hatches.

### SEC-3 — Decompression bomb / unbounded buffering (AUDIT-003) — **High**
- **Attacker capability:** any server, or MITM if SEC-2 is on.
- **Precondition:** response with `Content-Encoding: gzip` (client always advertises gzip/br/zstd, `compression.rs:71-78`).
- **Impact:** OOM of the embedding process from a KB-sized payload (1000:1 expansion). Also reachable with legitimately huge collections (no cap exists at all).
- **Mitigation:** `max_response_body_size` guard in `decompress_body`/`decompress_stream` (`compression.rs:187-238`). Note: a prior fix plan already specified this (`docs/superpowers/plans/2026-08-20-audit-fixes.md` Task 4) and was never executed (AUDIT-009).

### SEC-4 — Client-side path escape via dot segments (AUDIT-023) — **Low**
- `build_uri` does not normalize `..` (`client.rs:262-314`): relative hrefs can climb out of the base collection prefix. Server ACLs are the real boundary; this removes a client-side safety net.
- **Mitigation:** RFC 3986 §5.2.4 dot-segment removal, or reject `..`.

### SEC-5 — Raw-XML escape hatches (documented injection surfaces) — **Informational**
- `carddav::addressbook_query(filter_xml, …)` splices caller XML verbatim (`src/carddav/client.rs:467-473` → `741-743`); `mkcalendar`/`mkaddressbook` take raw XML bodies by design (`caldav/client.rs:362-376`). Callers interpolating untrusted values into these have zero protection.
- **Assessment:** by-design power features; the structured alternatives (`CalendarQueryFilter`, `CardDavFilter::to_filter_xml`) escape every value (verified exhaustively — §2). Asymmetry note: CalDAV validates inputs pre-network, CardDAV's raw path does not (AUDIT-006 divergence table).

### SEC-6 — Supply chain — **Low**
- `cargo audit`: **0 advisories** (141 deps, 2026-08-31).
- Gaps: no action SHA-pinning, no `cargo-deny` (license/ban scanning), docker-compose binary fetched without checksum (`e2e-tests.yml:47-51`) (AUDIT-019).
- `.env` not gitignored (AUDIT-030) — hygiene; no secret currently committed (verified `.envrc`, `flake.nix`, `sabredav-test/**`).
- LGPL-3.0 license friction (AUDIT-031) — compliance/adoption, not a vulnerability.

## 2. Verified-secure areas (no action needed)

- **XML value injection: no breakout path.** All 23 dynamic interpolation sites audited; every field carrying semantic user data (filter names, hrefs, text-match values, sync tokens, datetimes, component names) is escaped (`escape_xml`, `webdav/xml.rs:3-16`) and/or validated (`validate_component_name`, `validate_utc_datetime`). Unescaped slots are structural constants and the documented raw-XML APIs (SEC-5).
- **Header/CRLF injection: impossible.** All dynamic headers pass `HeaderValue::from_str`/`from_static`; ETag opaque validation rejects quotes and control chars (`client.rs:79-93, 103-105`); `If-None-Match` only ever static `"*"`.
- **XXE / entity expansion: impossible.** quick-xml does not resolve external entities; DOCTYPE events skipped; unknown entities → hard error. Verified against quick-xml-0.41.0 source.
- **Secrets lifecycle:** `zeroize` genuinely used (builder `Drop`, auth-header construction, base64 intermediates); hand-written redacting `Debug` on the builder (tested); `WebDavClient` derives no `Debug`. Residual (accepted, documented at `client.rs:129-137`): the `Authorization` HeaderValue lives for the client's lifetime; two intermediate `Bearer …`/proxy strings not zeroized (`builder.rs:373, 534-535`) — low value, note only.
- **CI:** no `pull_request_target` in any workflow; least-privilege `permissions: contents: read`; publish dry-run present (gate hole = AUDIT-005).
- **Lock ordering** (compression state): consistent order, no ABBA deadlock; poisoning recovered at all 11 sites; only Copy values shared.

## 3. Priority

1. SEC-1 origin check (small diff, closes the only credential-egress path).
2. SEC-3 size caps (client libs get embedded into long-lived processes; OOM is the realistic worst case).
3. SEC-2 loudness in release builds.
4. SEC-6 CI hardening (SHA-pinning, cargo-deny).
