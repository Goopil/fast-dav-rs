# Contributing to fast-dav-rs

Thank you for your interest in contributing to fast-dav-rs!

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/Goopil/fast-dav-rs.git`
3. Create a new branch: `git checkout -b my-feature-branch`
4. Make your changes
5. Run the checks below (all of them run in CI)
6. Submit a pull request

## Checks (all mandatory, same as CI)

```bash
# Formatting — must produce no diff
cargo fmt --all --check

# Lint — strict: any warning fails the build
cargo clippy --all-targets --all-features -- -D warnings

# Unit tests (nextest; equivalent: cargo test --all-features --test unit_tests)
cargo nextest run --all-features --locked --test unit_tests

# Doc tests — every Rust snippet in README.md, docs/*.md, and the API docs must compile and run
cargo test --doc --all-features

# Examples must build
cargo build --examples --all-features --locked
```

E2E tests run against Docker fixtures (SabreDAV, Radicale, Nextcloud) — see
`docs/e2e-testing.md` and the README in each `*-test/` directory.

## Quality Gates (SonarCloud, enforced on every PR)

1. **Coverage on new code ≥ 80%** — unit-test your new lines. Code reachable
   only through e2e tests against a live DAV server is exempt; say so in the
   PR if a gate fails for that reason.
2. **Duplications on new code ≤ 3%** — do not copy-paste between `caldav/`
   and `carddav/`; share logic via `webdav/` or `common/` instead.

These gates must not be bypassed.

## Code Style

- `rustfmt` + strict `clippy`, as above
- Typed errors: use the `Error` enum in `src/error.rs`; add specific variants
  rather than `Error::other` when a case is worth matching on
- All public APIs need doc comments, including error conditions; doc examples
  are doctests and must pass
- Keep `README.md`, `AGENTS.md`, and `examples/` in sync when you add,
  remove, or change public APIs or configuration — stale documentation is a
  bug
- No TODO/FIXME comments in final code
- Write clear, concise commit messages focused on the why

## API Stability (semver)

Every pull request is gated by `cargo-semver-checks` (the `Semver Checks` workflow), which compares
the public API against the latest published release with all features enabled and fails on breaking changes.

Intentional breaking changes:

- Land in a `0.x` minor-version bump (e.g. `0.14.0` -> `0.15.0`).
- Use deprecation aliases where feasible so callers can migrate gradually.
- Must be called out in `CHANGELOG.md`.

## Testing

- Include tests for new functionality: happy path *and* error cases
- Unit tests live in `tests/unit/` (per-module subdirectories)
- E2E tests live in `tests/e2e/`, one subtree per fixture
- Code coverage is maintained or improved

## Reporting Issues

Please use the GitHub issue tracker to report bugs or suggest features:

- Check if the issue already exists
- Provide a clear description of the problem
- Include steps to reproduce for bug reports
- Specify your environment (OS, Rust version, etc.)

## Pull Request Process

1. Ensure all checks and quality gates pass
2. Update documentation as needed (including README and examples)
3. Describe your changes in the PR description and link related issues
4. Be responsive to feedback during review

## Questions?

Open an issue on the [issue tracker](https://github.com/Goopil/fast-dav-rs/issues)
or start a [discussion](https://github.com/Goopil/fast-dav-rs/discussions).
