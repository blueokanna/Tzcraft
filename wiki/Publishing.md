# Publishing (crates.io + docs.rs)

Notes for the maintainer. The repository is already configured for both
platforms; this page documents the configuration and the release procedure.

## Cargo metadata

`Cargo.toml` carries the fields crates.io and docs.rs need:

```toml
[package]
name = "tzcraft"
version = "0.1.1"
edition = "2021"
authors = ["blueokanna"]
license = "Apache-2.0"
description = "A schema-driven date & time library: one 128-bit nanosecond timeline, a const civil calendar, and codec-aware wire formats on nextjson / rustbinary."
repository = "https://github.com/blueokanna/Tzcraft"
documentation = "https://docs.rs/tzcraft"
readme = "README.md"
keywords = ["date", "time", "datetime", "timezone", "posix"]
categories = ["Date and Time", "Value Formatting and Parsing", "Data Processing"]

[package.metadata.docs.rs]
all-features = true
rustdoc-args = ["--cfg", "docsrs"]
```

- `readme = "README.md"` — the English README is the default on crates.io and
  docs.rs; `README_CN.md` links back from it.
- `documentation = "https://docs.rs/tzcraft"` — docs.rs is the canonical docs
  host.
- `license = "Apache-2.0"` with a full `LICENSE` file in the repo.
- `keywords` and `categories` are chosen from the crates.io whitelists.
- MSRV is declared with `rust-version = "1.81"`.

## docs.rs build

docs.rs builds with `--all-features` and the `--cfg docsrs` flag. The crate
mirrors that exact condition in CI (nightly + `RUSTDOCFLAGS="--cfg docsrs"`)
so a docs.rs failure is caught before a release. The `doc_cfg` attribute
(which absorbed `doc_auto_cfg` in Rust 1.92) renders the "Available on crate
feature X only" badges on feature-gated items.

Local verification:

```sh
# stable, catches intra-doc link and missing-doc issues
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps

# the exact docs.rs condition
RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc --all-features --no-deps
```

## Release checklist

1. Bump `version` in `Cargo.toml`; keep `rust-version` and the exact
   dependency pins in sync with what CI validates.
2. Run the full suite and the docs.rs-condition build locally (commands
   above).
3. `cargo package --list` to inspect the crate contents (verify no
   `target/` files, no `Cargo.lock` unless intended, both READMEs present).
4. `cargo publish --dry-run` (optional), then `cargo publish`.
5. After the docs.rs build finishes, confirm the crate page and the
   `migration` / `write` modules render correctly.

## Dependencies

Both dependencies are optional and exact-pinned:

```toml
nextjson = { version = "=0.1.4", features = ["derive"], optional = true }
rustbinary = { version = "=0.1.7", optional = true, features = ["std"] }
```

There is **no third-party date/time library** in the dependency graph: the
`migration` module is documentation only. Exact pins (`=`) keep the MSRV
and wire behaviour reproducible. They are deliberate: a caret range would
let a future release silently raise the MSRV or change behaviour that this
crate depends on.
