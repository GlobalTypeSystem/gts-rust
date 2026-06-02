//! Test helpers for exercising GTS validation from crates that author schemas
//! with `gts-macros`.
//!
//! The primary entry point is [`validate_traits_chain`], which runs the registry
//! OP#13 trait validation over a derivation chain of macro-generated schemas
//! without the caller having to wire up a [`GtsOps`] by hand.

use serde_json::Value;

use crate::ops::GtsOps;

/// Register a base→leaf chain of GTS type schemas and run OP#13 trait validation
/// on the leaf.
///
/// Each schema except the last is registered with `validate = false` (an
/// intermediate whose required traits may legitimately be closed by a
/// descendant); the last with `validate = true`, triggering the chain-aggregated
/// trait completeness / merge check against the fully composed chain.
///
/// # Errors
/// Returns an error if `chain` is empty, or the registry error string of the
/// first schema that fails to register (or fails validation).
pub fn validate_traits_chain(chain: &[&Value]) -> Result<(), String> {
    if chain.is_empty() {
        return Err("validate_traits_chain: empty chain (no schemas to validate)".to_owned());
    }
    let mut ops = GtsOps::new(None, None, 0);
    let last = chain.len().saturating_sub(1);
    for (i, schema) in chain.iter().enumerate() {
        let validate = i == last;
        let result = ops.add_entity(schema, validate);
        if !result.ok {
            return Err(result.error);
        }
    }
    Ok(())
}

/// Register an arbitrary set of GTS type schemas, then run full schema
/// validation (OP#12 chain compatibility + OP#13 trait-schema / trait-value
/// validation) on every one of them.
///
/// Unlike [`validate_traits_chain`], this does not assume a single linear host
/// chain. All schemas register first with `validate = false` (so order is
/// irrelevant and `$ref`s between them resolve), then each is validated. Use it
/// when a set of macro-generated schemas (trait types, hosts, intermediates)
/// must *all* be valid and mutually consistent in one registry.
///
/// # Errors
/// Returns an error if `schemas` is empty, or the error of the first schema that
/// fails to register or validate, prefixed with its `$id`.
pub fn validate_all(schemas: &[&Value]) -> Result<(), String> {
    if schemas.is_empty() {
        return Err("validate_all: empty schema set (nothing to validate)".to_owned());
    }
    let mut ops = GtsOps::new(None, None, 0);

    for schema in schemas {
        let result = ops.add_entity(schema, false);
        if !result.ok {
            return Err(result.error);
        }
    }

    for schema in schemas {
        let Some(id) = schema.get("$id").and_then(Value::as_str) else {
            return Err("schema is missing a string `$id`".to_owned());
        };
        let gts_id = id.strip_prefix("gts://").unwrap_or(id);
        let result = ops.validate_schema(gts_id);
        if !result.ok {
            return Err(format!("{gts_id}: {}", result.error));
        }
    }

    Ok(())
}
