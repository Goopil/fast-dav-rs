# CI Speedup via Prebuilt GHCR Images — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cut PR CI wall time ~50% by prebuilding the Rust toolchain + compiled dependencies + DAV fixtures into GHCR images, and removing workflow duplication.

**Architecture:** A `docker-images.yml` workflow builds 4 images pushed to `ghcr.io/goopil/fast-dav-rs/*` (triggers: paths / weekly / manual). Cargo-only jobs run in `container:` with a warm target dir copied out of the image; e2e jobs (host runners, need Docker) pull fixture images. `codspeed.yml`, `fuzz.yml`, `publish.yml` untouched.

**Images:** `ci-base`, `sabredav-app`, `sabredav-nginx`, `nextcloud-preinstalled` (tag `:latest`), GHA layer cache, public packages (repo is public).

## Global Constraints

- GHCR image paths must be lowercase: `ghcr.io/goopil/fast-dav-rs/...` (hardcoded).
- Cargo toolchains: stable (floating, `rust:1-bookworm`) + `1.85.0` MSRV pinned.
- Warm target baked at the exact GHA container workspace path `/__w/fast-dav-rs/fast-dav-rs`; `CARGO_INCREMENTAL=0` everywhere.
- Fallback if a job regresses: remove `container:` on that job and restore `Swatinem/rust-cache` (one-line, per-job revert).
- SonarCloud gates must keep passing: coverage on new code, no CalDAV/CardDAV duplication.

---

### Task 1: `docker/ci/ci-base.Dockerfile` (toolchain + tools + warm target)

**Files:**
- Create: `docker/ci/ci-base.Dockerfile`

- [ ] **Step 1: Write the Dockerfile**

```dockerfile
FROM rust:1-bookworm

ENV CARGO_INCREMENTAL=0

# Stable components + pinned MSRV toolchain (msrv job selects via RUSTUP_TOOLCHAIN)
RUN rustup component add llvm-tools-preview \
 && rustup toolchain install 1.85.0 --profile minimal

# CI tools (versions float with weekly image rebuild; --locked for reproducibility)
RUN cargo install --locked cargo-nextest \
 && cargo install --locked cargo-llvm-cov \
 && cargo install --locked cargo-codspeed

# Warm target built at the exact GHA container workspace path so fingerprints match
WORKDIR /__w/fast-dav-rs/fast-dav-rs
COPY Cargo.toml Cargo.lock ./

# Stub the crate: only dependencies compile; the crate recompiles per-PR.
RUN mkdir -p src tests/unit benches examples \
 && printf 'pub fn stub() {}\n' > src/lib.rs \
 && printf 'fn main() {}\n' > src/main.rs \
 && touch tests/unit/mod.rs \
 && printf 'fn main() {}\n' > benches/performance.rs \
 && printf 'fn main() {}\n' > benches/hot_paths.rs \
 && printf 'fn main() {}\n' > examples/stub.rs

# Warm: dev codegen (tests/examples/benches), clippy check artifacts,
# instrumented artifacts (coverage), MSRV check artifacts — one target/.
RUN cargo build --all-features --locked --tests --examples --benches \
 && cargo clippy --all-targets --all-features --locked -- -D warnings \
 && RUSTFLAGS="-Cinstrument-coverage" cargo build --all-features --locked --tests \
 && RUSTUP_TOOLCHAIN=1.85.0 cargo check --all-features --locked

RUN cp -a target /opt/warm-target
```

- [ ] **Step 2: Validate locally**

Run: `docker build -f docker/ci/ci-base.Dockerfile -t ci-base:local .`
Expected: builds clean (long, ~15-20 min cold).

### Task 2: `nextcloud-test/Dockerfile` (preinstalled instance)

**Files:**
- Create: `nextcloud-test/Dockerfile`

- [ ] **Step 1: Write the Dockerfile**

```dockerfile
FROM nextcloud:stable-apache

ENV SQLITE_DATABASE=nextcloud \
    NEXTCLOUD_ADMIN_USER=admin \
    NEXTCLOUD_ADMIN_PASSWORD=admin-password \
    NEXTCLOUD_TRUSTED_DOMAINS=localhost

# Bake the first-boot install at build time; the entrypoint then skips it.
RUN php occ maintenance:install \
      --database sqlite --database-name nextcloud \
      --admin-user admin --admin-pass admin-password \
      --admin-email admin@example.local \
 && OC_PASS=fixture-dav-password php occ user:add --password-from-env test
```

- [ ] **Step 2: Validate locally**

Run: `docker build -f nextcloud-test/Dockerfile -t nc-pre:local nextcloud-test/ && docker run -d -p 8083:80 nc-pre:local`
Expected: `status.php` responds in seconds; authenticated PROPFIND on principal returns 207. `setup.sh` unchanged (already tolerates an existing user).

### Task 3: `.github/workflows/docker-images.yml` (build + push)

**Files:**
- Create: `.github/workflows/docker-images.yml`

- [ ] **Step 1: Write the workflow**

4 parallel jobs (`ci-base`, `sabredav-app`, `sabredav-nginx`, `nextcloud-preinstalled`), each `docker/build-push-action@v7` with `push: true`, `tags: ghcr.io/goopil/fast-dav-rs/<name>:latest`, `cache-from/to: type=gha,scope=<name>,mode=max`. Triggers: push to main (paths `docker/**`, `sabredav-test/**`, `nextcloud-test/**`, `Cargo.toml`, `Cargo.lock`, self), weekly cron Monday 04:00 UTC, `workflow_dispatch` (also used for branch validation via `--ref`). SabreDAV reuses existing Dockerfiles as build contexts. `permissions: packages: write`, `timeout-minutes: 40`.

- [ ] **Step 2: One-time manual step**

Make the 4 GHCR packages public after first push (GitHub → Packages → settings).

### Task 4: `e2e-tests.yml` — consume GHCR fixtures + cleanup

**Files:**
- Modify: `.github/workflows/e2e-tests.yml`

- [ ] Delete job `unit-tests` (duplicate of `ci.yml` nextest, and weaker: no `--all-features`)
- [ ] Delete the 3 "Install docker compose V2" steps + the `setup-buildx` step (preinstalled/unneeded)
- [ ] Job `e2e-tests`: replace the 2 `build-push-action` steps with `docker pull ghcr.io/goopil/fast-dav-rs/sabredav-app:latest` and `.../sabredav-nginx:latest`
- [ ] Replace `sleep 30` with a curl retry loop (24 × 5s; accept 200/401/207)
- [ ] Job `e2e-nextcloud`: remove `docker pull nextcloud:stable-apache` (compose pulls the GHCR image), `timeout-minutes: 20`
- [ ] rust-cache on e2e jobs **kept** (host runners)

### Task 5: `ci.yml` / `coverage.yml` / `semver-checks.yml` — ci-base container

**Files:**
- Modify: `.github/workflows/ci.yml`, `.github/workflows/coverage.yml`, `.github/workflows/semver-checks.yml`

- [ ] Common pattern:

```yaml
    container: ghcr.io/goopil/fast-dav-rs/ci-base:latest
    steps:
      - uses: actions/checkout@v7
      - name: Restore warm dependency build
        run: cp -a /opt/warm-target/. "$GITHUB_WORKSPACE/target/"
      # ... cargo steps unchanged, minus dtolnay/rust-toolchain, Swatinem/rust-cache, taiki-e installs
```

- [ ] `ci.yml` lint-and-test: `cargo package --locked` → `cargo package --locked --no-deps` (packaging checks remain; compile verification stays in publish.yml). msrv: add `env: RUSTUP_TOOLCHAIN: "1.85.0"` on the check step. audit job: unchanged (no container).
- [ ] `coverage.yml`: instrumented llvm-cov build reuses instrumented deps baked in the image (same `-Cinstrument-coverage` RUSTFLAGS). SonarCloud (node) works in container. Keep `fetch-depth: 0`.
- [ ] `semver-checks.yml`: keep `rust-toolchain: manual`.
- [ ] Fallback documented: per-job revert to non-container + rust-cache if a fingerprint mismatch regresses a job.

### Task 6: Docs + final validation

**Files:**
- Modify: `AGENTS.md`, `sabredav-test/README.md`, `nextcloud-test/README.md`

- [ ] `AGENTS.md`: CI section — Docker Images workflow, manual rebuild (`gh workflow run docker-images.yml --ref <branch>`), which jobs consume which images.
- [ ] Fixture READMEs: GHCR image is the default; local build still possible (compose keeps `build:` keys).
- [ ] Validation sequencing: push branch → `gh workflow run docker-images.yml --ref ci/prebuilt-images` → wait for the 4 images → open PR → check green: ci, coverage, semver, e2e ×3 + before/after timings in the PR.

**Out of scope (YAGNI):** nightly-fuzz image, codspeed in container (specific instrumented profiles, low gain), docs-only paths filters.
