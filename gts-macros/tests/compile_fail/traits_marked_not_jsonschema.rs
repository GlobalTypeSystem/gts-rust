//! `#[derive(GtsTraitsSchema)]` on a struct that is missing
//! `#[derive(JsonSchema)]` must fail at compile time: the derive emits
//! `impl gts::GtsTraitsSchema`, whose `schemars::JsonSchema` supertrait is
//! unsatisfied. The error lands on the struct itself, before it is ever
//! referenced in `inline(...)` — exactly like `#[derive(Eq)]` without
//! `PartialEq`.

use gts_macros::GtsTraitsSchema;

// Marked, but no `#[derive(schemars::JsonSchema)]`.
#[derive(GtsTraitsSchema)]
pub struct MarkedButNotJsonSchema {
    pub retention: String,
}

fn main() {}
