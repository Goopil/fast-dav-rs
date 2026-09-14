# CI base image for fast-dav-rs: pinned toolchains, CI tools, and a
# pre-compiled dependency tree (warm target). Rebuilt by docker-images.yml
# (paths on Cargo.lock/toolchain files + weekly cron).
#
# The warm target is built at the exact GitHub Actions container workspace
# path (/__w/<repo>/<repo>) so artifact fingerprints match on CI; jobs copy
# it out to $GITHUB_WORKSPACE/target and only the crate itself recompiles.
# CARGO_INCREMENTAL=0 keeps artifacts relocatable (no incremental caches).
FROM rust:1-bookworm

ENV CARGO_INCREMENTAL=0

# Stable components + pinned MSRV toolchain (msrv job selects via RUSTUP_TOOLCHAIN)
RUN rustup component add llvm-tools-preview \
 && rustup toolchain install 1.85.0 --profile minimal

# CI tools (versions float with weekly image rebuild; --locked for reproducibility)
RUN cargo install --locked cargo-nextest \
 && cargo install --locked cargo-llvm-cov \
 && cargo install --locked cargo-codspeed

# Warm target: stub the crate so only dependencies compile; the crate itself
# is recompiled per-PR from real sources.
WORKDIR /__w/fast-dav-rs/fast-dav-rs
COPY Cargo.toml Cargo.lock ./

# Stubs matching every target declared in Cargo.toml ([[test]], [[bench]], examples).
RUN mkdir -p src tests/unit tests/e2e/sabredav tests/e2e/radicale \
             tests/e2e/nextcloud tests/e2e/provider_a benches examples \
 && printf 'pub fn stub() {}\n' > src/lib.rs \
 && printf 'fn main() {}\n' > src/main.rs \
 && touch tests/unit/mod.rs \
          tests/e2e/sabredav/mod.rs \
          tests/e2e/radicale/mod.rs \
          tests/e2e/nextcloud/mod.rs \
          tests/e2e/provider_a/mod.rs \
 && printf 'fn main() {}\n' > benches/performance.rs \
 && printf 'fn main() {}\n' > benches/hot_paths.rs \
 && printf 'fn main() {}\n' > examples/stub.rs

# Warm: dev codegen (tests/examples/benches), clippy check artifacts,
# instrumented artifacts for coverage (-Cinstrument-coverage, matching
# cargo-llvm-cov), and MSRV check artifacts — all in one target/.
RUN cargo build --all-features --locked --tests --examples --benches \
 && cargo clippy --all-targets --all-features --locked -- -D warnings \
 && RUSTFLAGS="-Cinstrument-coverage" cargo build --all-features --locked --tests \
 && RUSTUP_TOOLCHAIN=1.85.0 cargo check --all-features --locked

RUN cp -a target /opt/warm-target
