# Implementation Plan: Macro Migration

This plan realizes [ADR-001](./001-gts-schema-derive-macro-adr.md) and the coexistence story in [002-struct-to-gts-schema-migration.md](./002-struct-to-gts-schema-migration.md).

The foundation of the new macro is already in place on branch `gts-macro-implementation`: derive entry point, attribute parsing, validation, code generation, serde bridge, and a full v2 test matrix (~104 passing tests across 7 suites plus a 24-fixture trybuild suite). This plan covers the remaining work to bring the implementation into exact alignment with the ADR and make the new API the documented one.

Both macros continue to coexist at the end of this plan. Migrating callers off `#[struct_to_gts_schema]` is a separate, broader effort that has to be coordinated across the ecosystem; deprecation and removal of the old macro are deferred until that migration is complete and are explicitly out of scope here.

Each phase is a gate — all tasks and acceptance criteria must be green before the next phase begins. Phases produce independently shippable commits.

---

## Current status at a glance

### Done

- `#[derive(GtsSchema)]` entry point + `#[gts(...)]` struct and field attribute parsing (`gts_attrs.rs`, `gts_field_attrs.rs`)
- All validations carried over from the old macro: schema_id format, version match, segment count, generics, struct shape (`gts_validation.rs`)
- `GtsSchema` trait impl, runtime API, associated constants (`gts_codegen.rs`)
- `"x-gts-ref": "/$id"` override on `#[gts(type_field)]` / `#[gts(instance_id)]` fields
- `description` emitted in runtime schemas (both root and derived paths)
- `GtsSerialize` / `GtsDeserialize` bridge + direct-serde blocking for nested structs (`gts_serde.rs`)
- Per-field serde rename in nested deserializer (fixes pre-existing `rename_all = "snake_case"` bug)
- 7 v2 test suites plus trybuild, all passing (counts as of Phase 3 exit): 24 compile-fail, 28 integration, 14 inheritance, 11 serialization, 5 serde-rename, 17 parity, 5 mixed-macro interop

### Open (this plan)

| # | ADR section | Gap | Phase |
|---|---|---|---|
| 1 | Serde Policy (“no opt-out”) | `allow_direct_serde` attribute exists in code | Phase 1 |
| 2 | Required Identity Field | Mandatory presence of `type_field`/`instance_id` not enforced (only mutex + duplicates) | Phase 1 |
| 3 | Struct-Level `#[gts(...)]` Attributes (“`extends = None` equivalent explicit form”) | Parser does not recognize `extends = None` | Phase 1 |
| 4 | Generated Constructor | `new(...)` constructor not generated | Phase 2 |
| 5 | Traceability / migration | README not yet rewritten for the new API | Phase 3 |

---

## Phase 1: ADR alignment — divergence cleanup

**Goal.** Bring the implementation into exact alignment with ADR-001. After this phase the compile-fail surface and attribute grammar match the ADR letter-for-letter.

**Why first.** Downstream callers will begin using the new macro as soon as we announce migration. Any attribute the macro accepts now (e.g., `allow_direct_serde`) that the ADR forbids will either need a deprecation cycle of its own or will silently confuse users. Cleanup before rollout.

### 1.1 Remove `allow_direct_serde` opt-out

ADR §Serde Policy: *“There is no opt-out attribute; testing and debugging paths must go through the base struct.”* The current implementation accepts `allow_direct_serde` on `#[gts(...)]` and threads it through validation and serde generation.

- [x] Remove `allow_direct_serde: bool` from `GtsAttrs` (`gts-macros/src/gts_attrs.rs`)
- [x] Remove the `"allow_direct_serde"` match arm from `parse_inner`
- [x] Drop `allow_direct_serde` from the “unknown attribute” expected-keys message
- [x] Remove `validate_allow_direct_serde` from `gts_validation.rs` and its call site in `validate_all`
- [x] Remove any `allow_direct_serde`-conditional branch in `gts_serde.rs` so blocking is unconditional for `extends` structs
- [x] Delete `gts-macros/tests/v2_compile_fail/allow_direct_serde_on_root.{rs,stderr}`
- [x] Confirm `nested_direct_serialize` and `nested_direct_serialize_cfg_attr` compile-fail tests still pass — they are now the load-bearing guarantee

**Gate.** `cargo test -p gts-macros` green. `grep -rn allow_direct_serde gts-macros/` returns no matches.

### 1.2 Enforce mandatory identity field on root structs

ADR §Required Identity Field (root structs): *“Every root struct (no `extends`) must declare exactly one of `#[gts(type_field)]` or `#[gts(instance_id)]`. […] A root struct with neither is rejected at compile time.”* Derived structs (`extends = Parent`) must **not** carry an identity field — the root's chained identifier is already sufficient, and a redundant identity field on a nested struct produces an ambiguous serialized shape.

The current `validate_field_gts_attrs` rejects duplicates and the both-together combination, but neither enforces the zero-case on root structs nor the no-identity-on-derived rule.

- [x] Extend `validate_field_gts_attrs` (`gts-macros/src/gts_validation.rs`) to take `attrs: &GtsAttrs` so it can distinguish root from derived
- [x] On a root struct (`attrs.extends.is_none()`), error when neither `has_type_field` nor `has_instance_id` was observed
- [x] On a derived struct (`attrs.extends.is_some()`), error if either `has_type_field` or `has_instance_id` was observed (identity is root-only)
- [x] Error messages must name both valid annotations and point at the struct, not an arbitrary field
- [x] Audit every v2 test fixture: root structs carry one identity annotation; derived structs carry none. Fixtures where the old no-identity pattern was tested (`base_struct_no_gts_identity_field`, `data_entity_no_identity`, `no_gts_identity_field_roundtrip`) are updated to carry a `#[gts(type_field)]` identity field — the former "no identity" behavior is no longer valid per ADR
- [x] Add `v2_compile_fail/missing_identity_annotation.{rs,stderr}` — root struct with no `#[gts(type_field)]` or `#[gts(instance_id)]`
- [x] Add `v2_compile_fail/duplicate_instance_id.{rs,stderr}` — two fields annotated `#[gts(instance_id)]` (symmetry with existing `duplicate_type_field`)
- [x] Add `v2_compile_fail/identity_on_derived_struct.{rs,stderr}` — derived struct with `#[gts(type_field)]`, rejected

**Gate.** `cargo test -p gts-macros` green. Three new compile-fail tests pass via trybuild.

### 1.3 Support `extends = None` explicit form

ADR §Struct-Level Attributes: *“Absent means root; `extends = None` is an equivalent explicit form for grep/IDE discoverability.”* Current parser does `let ident: syn::Ident = input.parse()?;` which would accept the literal identifier `None` but then pass it downstream as a real parent type — wrong.

- [x] In `gts_attrs.rs::parse_inner`, when parsing the `"extends"` key, check if the parsed ident is `None`, and in that case leave `extends` as `Option::None` (same as absent)
- [x] Otherwise parse as `syn::Ident` for the parent type
- [x] Add integration test `extends_none_equals_absent`: assert that a struct with `extends = None` and a struct with no `extends` produce structurally equal `gts_schema_with_refs()` output
- [x] Add `v2_compile_fail/extends_none_multi_segment.{rs,stderr}` — `extends = None` with a multi-segment `schema_id` errors identically to the absent case

**Gate.** `cargo test -p gts-macros` green including the new behavioral test. Compile-fail suite still targets the same error semantics.

### Phase 1 exit criteria

- All ADR attribute grammar divergences closed
- Full test suite green (now ~104 tests across 7 v2 suites plus a 24-fixture trybuild suite)
- `cargo fmt --all` applied (formatting changes committed alongside the phase work, not in a follow-up)
- `make clippy` clean (runs `cargo clippy --workspace --all-targets --all-features -- -D warnings`)
- Commit message references ADR-001 §Serde Policy, §Required Identity Field, §Struct-Level Attributes

---

## Phase 2: Generated `new(...)` constructor

**Goal.** Generate a `new(...)` constructor that auto-populates the identity field, as specified in ADR §Generated Constructor.

**Why this matters.** The current design still allows callers to hand-write struct literals and assign the wrong value to the identity field — exactly the error surface the ADR calls out as motivation for the constructor. Until this ships, Issue #72’s root fix (identity field is mandatory, safe by construction) is only half-done.

### 2.1 Constructor codegen

- [x] Add a new function `gen_constructor` in `gts_codegen.rs` that emits an `impl` block with `pub fn new(...) -> Self`
- [x] Constructor signature: every named field in struct-definition order **except** any field annotated `#[gts(type_field)]` (which is auto-populated). `#[gts(skip)]` and `#[serde(skip)]` are schema/serde concerns and do **not** affect the constructor — every field is still part of the struct's data model and is passed through normally. This diverges from the earlier plan wording about `Default::default()` substitution and simplifies the generated code
- [x] Identity-field population:
  - [x] If the struct has a generic type parameter `P: GtsSchema`, populate `#[gts(type_field)]` from `::gts::gts::GtsSchemaId::new(<P as ::gts::GtsSchema>::SCHEMA_ID)` (the trait's `SCHEMA_ID` constant; no `gts_schema_id()` trait method exists)
  - [x] Otherwise populate `#[gts(type_field)]` from `Self::gts_schema_id().clone()` (the inherent LazyLock-cached accessor generated alongside)
  - [x] For `#[gts(instance_id)]` structs, the identity field is passed by the caller as a normal parameter (macro does not synthesize the instance segment)
- [x] Constructor emitted for both generic and non-generic structs; impl generics carry the `GtsSchema + JsonSchema` bound via `info.gts_schema_where`
- [x] Constructor not emitted for unit structs or structs with zero named fields

### 2.2 Interaction with existing code

- [x] Verified no existing in-repo struct derives `GtsSchema` and also writes its own `pub fn new(...)` — there are no collisions in the tree today. A user who later defines their own `new` will get rustc's standard "duplicate definitions for `new`" error, which we treat as acceptable (and documented in the rustdoc on the generated method).
- [x] Confirmed the constructor respects serde renames — it operates on Rust field identifiers, so `#[serde(rename)]` is irrelevant to the constructor API. The roundtrip test in 2.3 exercises renamed fields (`OrderV1_0.gts_type` with `#[serde(rename = "type")]`) and confirms serialized output is identical whether built via `::new` or a struct literal.

### 2.3 Tests

- [x] `v2_integration_tests::generated_constructor_populates_type_field` — non-generic root, assert `Struct::new(...).gts_type` equals `Struct::gts_schema_id()`
- [x] `v2_integration_tests::generated_constructor_generic_populates_from_p` — generic root, assert the populated value comes from `P::SCHEMA_ID`, not the base struct's own
- [x] `v2_integration_tests::generated_constructor_for_instance_id` — root with `instance_id`, assert the id is passed through verbatim
- [x] `v2_integration_tests::generated_constructor_for_derived_struct` — derived struct has no identity, all fields in order
- [x] `v2_integration_tests::generated_constructor_respects_gts_skip` — `#[gts(skip)]` field still appears in the constructor signature
- [x] `v2_serialization_tests::constructor_instance_roundtrips` — constructor-built and struct-literal-built instances produce identical JSON; round-trip via Serialize → Deserialize is lossless
- [ ] `v2_parity_tests::constructor_produces_old_macro_equivalent` — **not added**: the old macro has no constructor, so there is no comparable old-macro output. The existing parity suite already asserts the structural equivalence of schemas and instance JSON; the new constructor only affects how the value is built in Rust, not what lands in JSON. Revisit if we ever port a constructor pattern backward.
- [ ] Optional compile-fail for user-written `new` collision — **not added**: a standard rustc duplicate-impl error would not validate anything macro-specific, and the cost of maintaining a trybuild fixture for a rustc-native error outweighs the value.

### Phase 2 exit criteria

- `new(...)` constructor generated on every `#[derive(GtsSchema)]` struct with fields
- All new tests pass; parity suite still green
- Migration guide §4.3 example now actually compiles (currently aspirational)
- `cargo fmt --all` applied
- `make clippy` clean
- Commit message references ADR-001 §Generated Constructor

---

## Phase 3: Rewrite the macro README

**Goal.** Make the new API the documented API. Both macros remain functional; the old macro is presented as legacy, and the README points users at the migration guide without emitting any compile-time deprecation.

### 3.1 Rewrite `gts-macros/README.md`

- [x] Replace old-macro examples with `#[derive(GtsSchema)]` examples mirroring ADR-001 §Entry Point and §Inheritance
- [x] Document every `#[gts(...)]` attribute (struct-level and field-level) with a one-line description and an example
- [x] Document the generated `new(...)` constructor from Phase 2
- [x] Cross-link to the ADR and migration guide
- [x] Add a top-of-file banner pointing callers of the old macro at the migration guide (prose only — no `#[deprecated]` attribute, to keep workspace and downstream builds quiet until the broader migration is ready)
- [x] Leave the old macro’s existing doc comments in place so `cargo doc` still renders them correctly for current users (no source change to `gts-macros/src/lib.rs` in this phase)

### 3.2 Confirm coexistence still holds

- [x] `cargo test -p gts-macros` green — both macros continue to pass their full test suites
- [x] `v2_inheritance_tests_mixed.rs` (5 tests) and `v2_parity_tests.rs` (17 tests) still green

### Phase 3 exit criteria

- README documents the new API in full and presents the old macro as legacy
- `cargo test -p gts-macros` green
- `cargo fmt --all` applied (covers any code snippets in the README that live inside doctests or examples)
- `make clippy` clean (README doctests, if any, respect the workspace lint config)
- No source change to `#[struct_to_gts_schema]` behavior or visibility

---

## Out of scope for this plan

The following are explicitly deferred — tracked separately if/when they become work:

- **Migrating existing `#[struct_to_gts_schema]` callers off the old macro**: a cross-cutting effort that has to be coordinated across every downstream consumer. This plan leaves both macros coexisting and functional; migration gets its own plan once it is ready to start.
- **Deprecating `#[struct_to_gts_schema]`** (`#[deprecated]` attribute, diagnostic banners in the macro body, CHANGELOG entries): strictly gated on the migration above. Turning it on prematurely would flood in-tree and downstream builds with warnings nobody can act on yet.
- **Removing `#[struct_to_gts_schema]` and its tests**: happens only after deprecation has shipped for at least one release and no external callers remain.
- **Schema traits** ([Spec §9.7](https://github.com/GlobalTypeSystem/gts-spec#97---schema-traits-x-gts-traits-schema--x-gts-traits)): `x-gts-traits-schema` / `x-gts-traits` emission. ADR §File Layout calls out that the module structure accommodates this as a future struct-level attribute.
- **PII / sensitivity annotations**: future field-level attribute following the same parse-validate-generate pipeline.
- **Enum support**: currently rejected outright. No ADR for this yet.
- **Multiple generic parameters**: currently rejected. Conflicts with the GTS single-chain inheritance model — would require a spec change before a macro change.

---

## Phase dependency graph

```
Phase 1 ──→ Phase 2 ──→ Phase 3
(align)    (ctor)      (README)
```

Each phase is a gate. Phase N+1 does not start until Phase N exit criteria are met and the phase is merged. At the end of Phase 3 both macros coexist and the new one is the documented default — migration and eventual retirement of the old macro are handled by a separate plan (see “Out of scope”).

---

## Traceability

- **ADR**: [001-gts-schema-derive-macro-adr.md](./001-gts-schema-derive-macro-adr.md)
- **Migration guide**: [002-struct-to-gts-schema-migration.md](./002-struct-to-gts-schema-migration.md)
- **Issue**: [#72 — `struct_to_gts_schema`: `gts_type` field blocks `Deserialize`](https://github.com/GlobalTypeSystem/gts-rust/issues/72) — resolved by Phase 1.2 and Phase 2 together (identity field is mandatory and safely populated)
- **Branch**: `gts-macro-implementation`
- **PR**: [#78](https://github.com/GlobalTypeSystem/gts-rust/pull/78)
