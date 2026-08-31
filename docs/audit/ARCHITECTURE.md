# Architecture Review — fast-dav-rs 0.9.0

**Audit date:** 2026-08-31 · Companion to `FINDINGS.md`.

## 1. Current architecture

```
                 ┌──────────── src/lib.rs (facade: 13 re-export groups + legacy cfg modules) ─────────────┐
                 ▼                                                                                         ▼
src/caldav/                 src/carddav/                 src/webdav/                      src/common/
  client.rs (859)  ──78%──► client.rs (1014)            client.rs (1535) ← god file      compression.rs (323)
  builder.rs (33)   100%*   builder.rs (33)             builder.rs (992)  ← god file     http.rs (59)
  streaming.rs (505) ─92%─► streaming.rs (505)          streaming.rs (712), xml.rs (198)  
  types.rs (446)            types.rs (334)              types.rs (162)                    src/error.rs (469)
        └──────── both delegate HTTP to WebDavClient ────────┘
```
\* via shared `impl_dav_builder!` macro — the model to replicate. Dependency direction is clean and acyclic: `caldav`/`carddav` → `webdav` → `common` → `error`; no reaching into privates; one intra-`webdav` client↔builder type cycle (Rust-idiomatic, contained).

## 2. Structural problems

### ARCH-1 — The duplication is the architecture (AUDIT-006, AUDIT-014)
caldav↔carddav: 78–92% duplicated code with **behavioral divergences** already shipped (MKCOL fallback, validation asymmetry, inline XML builders). Five types defined twice; crate-root re-exports bind only carddav's `TextMatch`/`Collation`/`MatchType`/`ParamFilter` and caldav's `DavItem`/`SyncItem`/`SyncResponse` — a name-collision trap for generic users. A dedup design spec exists (`docs/superpowers/specs/2026-08-20-dedup-macros-design.md`) and was never executed (AUDIT-009). **This is the repo's primary architectural time bomb:** every future feature (a third DAV dialect? per-property filtering? new report types) is built twice and fixed twice. SonarCloud's ≤3%-on-new-code gate survives only because the duplication is old code.

### ARCH-2 — God files in webdav (AUDIT-010 context, ARCH)
`webdav/client.rs` (1535 lines) carries ~10 responsibilities: HTTP verbs, WebDAV verbs, URI construction, auth attachment, request-compression state machine, probe, compression retry, decompression+header rewriting, bounded batch execution, capability detection+discovery, ETag utilities. `webdav/builder.rs` (992 lines) similarly mixes validation, zeroize lifecycle, TLS verifier, hyper construction. Neither is unmaintainable today, but both are at the size where every change risks unrelated fallout; the compression probe alone (state machine + mutex + retry + cache) would be one focused 200-line module.

### ARCH-3 — API asymmetry between the two clients (AUDIT-006, AUDIT-013, AUDIT-015)
CalDAV: typed, validated `CalendarQueryFilter` + timerange helper. CardDAV: raw-XML `addressbook_query` + typed `CardDavFilter`. Raw-XML injection surface on one side, validation on the other; `SyncResponse` not `#[non_exhaustive]` while `SyncItem` is; `ElementName`/`element_from_bytes` parser internals leaked `pub` (AUDIT-028). Generic code over both clients cannot share a query abstraction.

### ARCH-4 — No observability seam (AUDIT-010)
Zero `log`/`tracing` hooks. Architecturally this is a missing *extension point* the library owes its embedders; retrofitting after 1.0 would be a breaking-ish change (new feature/dep), so the window to add it cheaply is now.

## 3. Decisions that were right (keep)

- **Thin wrapper pattern**: `CalDavClient { webdav: WebDavClient }` — genuinely reused, not copy-pasted structurally.
- **`impl_dav_builder!` macro** generating both builders from one source — the proven in-repo answer to ARCH-1.
- **Shared `webdav/xml.rs` builders + `CommonParser`** — the dedup already started; finish it.
- **`#[non_exhaustive]` + public constructors** on the error enum; single shared `Error`/`Operation` taxonomy.
- **Stateless client, one pooled connection, cheap `Clone`.**
- **hyper-util legacy client, tower-service, quick-xml push parser** — boring, proven choices.

## 4. Target architecture (recommended, incremental)

1. **Phase A (no breaking change):** port the two divergence fixes (MKCOL fallback → caldav; validation → carddav raw path documented/deprecated). Extract `compression/probe.rs` and `discovery.rs` from `webdav/client.rs`.
2. **Phase B (0.10):** execute the dedup spec — parameterize `MultistatusParser` over an element-name trait (generic over a small `ProtocolElements` trait); move `TextMatch`/`Collation`/`MatchType`/`ParamFilter` to `webdav` with deprecation aliases at old paths; unify `DavItem` common fields (already 68–76% identical) behind `DavItemCommon` + `apply_common_fields!` (existing pattern).
3. **Phase C:** add the `tracing` feature seam (AUDIT-010); gate `CardDAV` raw-filter API behind a `raw-xml` feature or rename to `addressbook_query_raw` to make the injection surface explicit.
4. **Explicitly deferred:** a shared `DavClient<P: Protocol>` blanket abstraction unifying both clients behind generics — attractive, but the macro+shared-module route reaches 90% of the benefit at 20% of the risk. Revisit only if a third dialect appears.

## 5. Decisions to record (mini-ADRs)

- **ADR-1: dedup strategy** — macros + shared parameterized parser vs trait-based protocol abstraction vs full generics. *Recommended:* spec already written (`specs/2026-08-20-dedup-macros-design.md`); execute it; revisit generics only for a third dialect.
- **ADR-2: discovery caching** — library-owned cache vs caller-owned. *Recommended:* caller-owned (document), optional opt-in cached helpers (AUDIT-027). Auto-caching server topology risks stale-URL bugs after server-side moves — the library should not guess invalidation.
- **ADR-3: raw-XML APIs** — keep (power feature) but make the surface explicit (`*_raw` naming / feature gate) and align validation semantics with the typed paths.
