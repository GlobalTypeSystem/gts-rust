//! Build script for `gts-id`.
//!
//! `GTS_ID_PREFIX` is resolved at compile time via `option_env!`. Cargo does
//! not track `option_env!`/`env!` reads automatically, so without this hint a
//! stale build would silently keep the previously compiled prefix when the
//! environment variable changes. Emitting `rerun-if-env-changed` forces a
//! rebuild whenever the prefix is changed.
//!
//! A build script is a standalone program compiled before (and independently
//! of) the crate, so it cannot reference `gts_id::GTS_ID_PREFIX_ENV` directly.
//! The variable name is therefore hardcoded here; keep it in sync with
//! `GTS_ID_PREFIX_ENV` / the `option_env!` literal in `src/prefix.rs`.
fn main() {
    println!("cargo:rerun-if-env-changed=GTS_ID_PREFIX");
}
