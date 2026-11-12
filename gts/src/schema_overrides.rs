//! Ancestor field overrides for derived type schemas.
//!
//! A derived type may replace fields its ancestors declare (e.g. narrow a
//! root `id: string` to a UUID) through the `overrides(Ancestor = Helper)`
//! argument of `#[struct_to_gts_schema]`. The helper is a plain
//! `#[derive(JsonSchema)]` struct holding a subset of the ancestor's own
//! fields. Each helper field's schema is the field's complete new
//! declaration: it is emitted unchanged (annotations aside) into the derived
//! overlay at the ancestor's generic path, and no inherited keyword - nested
//! `properties` and `required` included - is merged into it.
//!
//! A replacement must accept no value the inherited field rejects. That is
//! decided by [`crate::schema_evolution::check_accepted_set_inclusion`], the
//! engine ordinary derivation admission uses, against *each* declaration of
//! the field along the parent's `allOf` chain: inclusion in every conjunct is
//! inclusion in their conjunction, and no conjunct is lost to a reduction. An
//! undecided comparison is a rejection. Requiredness follows the helper: an
//! optional field may become required, a required one cannot become optional.
//! The composed chain must then pass the registry's derivation admission
//! ([`validate_derivation_chain`]), whose rules inclusion alone does not
//! express (e.g. a closed nested object must restate every inherited field).
//! Emission and validation are separate: schema generation only applies the
//! declarations; generated tests and explicit validation check their validity.

use std::fmt;

use serde_json::{Map, Value};

use crate::schema::GtsSchema;

/// One `Ancestor = Helper` pair of an `overrides(...)` argument.
#[derive(Debug, Clone)]
pub struct AncestorOverride {
    /// The ancestor's type ID, for diagnostics.
    pub ancestor_type_id: &'static str,
    /// Generic-slot path from the document root to the ancestor's own fields.
    pub path: Vec<&'static str>,
    /// The ancestor's generic slot; it cannot be overridden.
    pub generic_field: Option<&'static str>,
    /// The helper's schema with every subschema inlined.
    pub helper: Value,
}

impl AncestorOverride {
    /// Builds the override `A = H`.
    #[must_use]
    pub fn of<A: GtsSchema, H: schemars::JsonSchema>() -> Self {
        Self {
            ancestor_type_id: A::TYPE_ID,
            path: A::outer_generic_path(),
            generic_field: A::GENERIC_FIELD,
            helper: helper_schema::<H>(),
        }
    }
}

/// `H`'s draft-07 schema with nested structs and enums inlined, so that no
/// `definitions` namespace leaks into the derived document. Only a recursive
/// type keeps a `$ref`, which [`validate_ancestor_overrides`] rejects.
#[must_use]
pub fn helper_schema<H: schemars::JsonSchema>() -> Value {
    let root = schemars::generate::SchemaSettings::draft07()
        .with(|settings| settings.inline_subschemas = true)
        .into_generator()
        .into_root_schema_for::<H>();
    serde_json::to_value(root).unwrap_or(Value::Null)
}

/// An override the inherited field does not admit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideError {
    /// Type ID of the ancestor named on the left of `=`.
    pub ancestor: String,
    /// Instance path of the offending field (JSON pointer).
    pub path: String,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for OverrideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "overrides for ancestor '{}': field '{}': {}",
            self.ancestor, self.path, self.message
        )
    }
}

impl std::error::Error for OverrideError {}

/// Resolves a type's own schema document against its parent's resolved one.
///
/// The `allOf` branch `{"$ref": "gts://<parent>"}` is replaced by `parent`,
/// and local `#/definitions/...` references (JSON pointers, `~0`/`~1`
/// escaping included) are inlined. Every `allOf` branch is kept. A reference
/// that cannot be inlined (recursive, missing target, or with assertion
/// siblings) keeps its `$ref` while `definitions` is dropped, so the result is
/// self-contained only when every local reference was inlined;
/// [`validate_ancestor_overrides`] rejects overriding a field that reaches a
/// leftover `$ref`.
#[must_use]
pub fn resolve_type_schema(own: &Value, parent: Option<Value>) -> Value {
    let mut schema = own.clone();
    let definitions = schema
        .as_object_mut()
        .and_then(|map| map.remove("definitions"))
        .unwrap_or_else(|| Value::Object(Map::new()));
    let root = serde_json::json!({ "definitions": definitions });
    inline_local_refs(&mut schema, &root, &mut Vec::new());

    if let Some(mut parent) = parent {
        if let Some(map) = parent.as_object_mut() {
            map.remove("$id");
            map.remove("$schema");
        }
        let parent_ref = own.get("$id").and_then(Value::as_str).and_then(parent_uri);
        if let (Some(branches), Some(parent_ref)) = (
            schema.get_mut("allOf").and_then(Value::as_array_mut),
            parent_ref,
        ) {
            for branch in branches {
                if branch.get("$ref").and_then(Value::as_str) == Some(parent_ref.as_str()) {
                    *branch = parent;
                    break;
                }
            }
        }
    }
    schema
}

/// `a~b~` -> `a~`, with or without the `gts://` prefix.
fn parent_uri(id: &str) -> Option<String> {
    let trimmed = id.strip_suffix('~')?;
    let cut = trimmed.rfind('~')?;
    Some(format!("{}~", &trimmed[..cut]))
}

/// Inlines local `$ref`s found in schema positions; literal data (`const`,
/// `enum`, `default`, `examples`, ...) is never interpreted as a schema.
///
/// Only a bare reference (`$ref` plus annotations) is replaced, by its target
/// as is (a boolean target stays boolean). A reference with assertion
/// siblings, a recursive one, or one whose target is missing keeps its `$ref`.
fn inline_local_refs(schema: &mut Value, root: &Value, stack: &mut Vec<String>) {
    for_each_subschema_mut(schema, &mut |subschema| {
        inline_local_refs(subschema, root, stack);
    });
    let Some(map) = schema.as_object() else {
        return;
    };
    let Some(reference) = map.get("$ref").and_then(Value::as_str).map(str::to_owned) else {
        return;
    };
    let bare = map
        .keys()
        .all(|keyword| keyword == "$ref" || ANNOTATIONS.contains(&keyword.as_str()));
    if !bare || stack.contains(&reference) {
        return;
    }
    let Some(mut target) = reference
        .strip_prefix('#')
        .and_then(|pointer| root.pointer(pointer))
        .cloned()
    else {
        return;
    };
    stack.push(reference);
    inline_local_refs(&mut target, root, stack);
    stack.pop();
    *schema = target;
}

/// Visits the direct subschemas of a draft-07 schema (the positions
/// `jsonschema::Draft::Draft7.subresources_of` reports, mutably).
fn for_each_subschema_mut(schema: &mut Value, visit: &mut dyn FnMut(&mut Value)) {
    let Some(map) = schema.as_object_mut() else {
        return;
    };
    for (keyword, value) in map.iter_mut() {
        match keyword.as_str() {
            "additionalItems"
            | "additionalProperties"
            | "contains"
            | "else"
            | "if"
            | "not"
            | "propertyNames"
            | "then"
            | crate::schema_traits::X_GTS_TRAITS_SCHEMA => visit(value),
            "items" => match value {
                Value::Array(items) => items.iter_mut().for_each(&mut *visit),
                other => visit(other),
            },
            "allOf" | "anyOf" | "oneOf" => {
                if let Value::Array(items) = value {
                    items.iter_mut().for_each(&mut *visit);
                }
            }
            "definitions" | "$defs" | "properties" | "patternProperties" | "dependencies" => {
                if let Value::Object(members) = value {
                    members
                        .values_mut()
                        .filter(|member| member.is_object() || member.is_boolean())
                        .for_each(&mut *visit);
                }
            }
            _ => {}
        }
    }
}

/// `true` if a `$ref` survives in a schema position of `schema`.
fn contains_ref(schema: &Value) -> bool {
    schema.get("$ref").is_some()
        || jsonschema::Draft::Draft7
            .subresources_of(schema)
            .any(contains_ref)
}

/// Validates the overrides of the derived type `type_id` without emitting a schema.
///
/// `parent` is the immediate parent's resolved schema
/// ([`resolve_type_schema`]). Every declaration of a field along its chain is
/// a baseline, whichever ancestor anchors the override, so an older anchor
/// cannot bypass a narrowing made by an intermediate level.
///
/// # Errors
///
/// The first override that is not proven included in an inherited
/// declaration, makes a required field optional, or addresses something a
/// helper cannot replace (an undeclared field, the generic slot, a recursive
/// type, an unsupported helper shape).
pub fn validate_ancestor_overrides(
    parent: &Value,
    type_id: &str,
    overrides: &[AncestorOverride],
) -> Result<(), OverrideError> {
    let context = Context {
        dialect: parent
            .get("$schema")
            .cloned()
            .unwrap_or_else(|| Value::String(crate::schema::JSON_SCHEMA_DRAFT_07.to_owned())),
        parent_id: crate::x_gts_pattern_ref::self_id(parent),
        type_id: type_id.to_owned(),
    };
    for each in overrides {
        validate_one(parent, &context, each)?;
    }
    Ok(())
}

/// What an extracted field schema loses by leaving its document.
struct Context {
    /// The parent document's `$schema`, which decides how keywords read.
    dialect: Value,
    /// Type selected by `/$id` operands in inherited declarations.
    parent_id: Option<String>,
    /// Type selected by `/$id` operands in replacements.
    type_id: String,
}

impl Context {
    /// `field` as a standalone document of `dialect`, with every `/$id`
    /// operand naming `selected`, the type its declaring document named:
    /// detached from that document, the operand would otherwise compare
    /// equal to any other `/$id`.
    fn standalone(&self, field: &Value, selected: Option<&str>) -> Value {
        let mut field = match selected {
            Some(selected) => {
                crate::x_gts_pattern_ref::with_self_operands_naming(field, selected).into_owned()
            }
            None => field.clone(),
        };
        if let Some(map) = field.as_object_mut() {
            map.insert("$schema".to_owned(), self.dialect.clone());
        }
        field
    }
}

/// Keywords a helper's root may carry. Everything else (combinators,
/// `definitions`, map-like or enum-like roots) has no field-wise meaning.
const HELPER_ROOT_KEYWORDS: &[&str] = &[
    "$schema",
    "type",
    "properties",
    "required",
    "additionalProperties",
];

fn validate_one(
    parent: &Value,
    context: &Context,
    entry: &AncestorOverride,
) -> Result<(), OverrideError> {
    let level_pointer: String = entry
        .path
        .iter()
        .flat_map(|segment| ["/", segment])
        .collect();
    let fail = |path: &str, message: String| OverrideError {
        ancestor: entry.ancestor_type_id.to_owned(),
        path: if path.is_empty() {
            "/".to_owned()
        } else {
            path.to_owned()
        },
        message,
    };

    let helper_properties = helper_fields(&entry.helper).map_err(|m| fail(&level_pointer, m))?;
    let helper_required = string_list(entry.helper.get("required"));

    let levels = declarations_at(parent, &entry.path).map_err(|m| fail(&level_pointer, m))?;
    if !levels.declared {
        return Err(fail(
            &level_pointer,
            "the ancestor level is missing from the parent's schema".to_owned(),
        ));
    }

    for (name, helper_field) in helper_properties {
        let pointer = format!("{level_pointer}/{name}");
        if entry.generic_field == Some(name.as_str()) {
            return Err(fail(
                &pointer,
                "is the ancestor's generic slot; constrain the derived payload with its own \
                 fields instead"
                    .to_owned(),
            ));
        }
        let mut field_path = entry.path.clone();
        field_path.push(name);
        let field = declarations_at(parent, &field_path).map_err(|m| fail(&pointer, m))?;
        if !field.declared {
            return Err(fail(&pointer, "is not declared by the ancestor".to_owned()));
        }
        let conjuncts = field.conjuncts;
        if conjuncts.iter().any(contains_ref) {
            return Err(fail(
                &pointer,
                "the inherited field reaches a recursive or unresolved `$ref`; it cannot be \
                 overridden"
                    .to_owned(),
            ));
        }
        let replacement = without_annotations(helper_field);
        if replacement == Value::Bool(false) {
            // Admission rejects a derivation that disables a base property;
            // fail here rather than at registration.
            return Err(fail(
                &pointer,
                "the helper field's schema is `false`, which would disable the inherited field"
                    .to_owned(),
            ));
        }
        check_included(&replacement, &conjuncts, context).map_err(|m| fail(&pointer, m))?;
        check_requiredness(name, &helper_required, &levels.conjuncts, context)
            .map_err(|m| fail(&pointer, m))?;
    }

    Ok(())
}

/// Runs the registry's derivation admission (OP#12) over `chain`, the
/// root-to-leaf type schemas of `type_id`: final bases and every adjacent
/// pair, exactly as [`crate::GtsStore::validate_schema`] does, without its
/// trait, reference or meta-schema checks.
///
/// [`validate_ancestor_overrides`] checks each replacement against the
/// declarations it replaces; this runs the registry's derivation admission
/// stage on the composed documents. It does not stand for the whole of
/// registration: trait, meta-schema and external reference checks are not run.
///
/// # Errors
///
/// The registry's rejection, attributed to `type_id`'s immediate parent.
pub fn validate_derivation_chain(type_id: &str, chain: &[Value]) -> Result<(), OverrideError> {
    let fail = |message: String| OverrideError {
        ancestor: parent_uri(type_id).unwrap_or_default(),
        path: "/".to_owned(),
        message: format!("the registry's derivation admission rejects the chain: {message}"),
    };
    let mut store = crate::GtsStore::new();
    for schema in chain {
        let id = crate::GtsStore::declared_type_id(schema).map_err(&fail)?;
        store
            .register_schema(&id, schema)
            .map_err(|e| fail(e.to_string()))?;
    }
    store
        .validate_schema_chain(type_id)
        .map_err(|e| fail(e.to_string()))
}

/// Emits helper fields at their ancestor paths without checking compatibility.
///
/// Only the helper's declared `properties` and `required` fields are emitted;
/// helper shape, field ownership and accepted-set inclusion are checked separately
/// by [`validate_ancestor_overrides`]. Invalid declarations can therefore be
/// inspected as schemas without a validation panic.
pub fn apply_ancestor_overrides(overlay: &mut Value, overrides: &[AncestorOverride]) {
    for entry in overrides {
        let mut target = &mut *overlay;
        for segment in &entry.path {
            let properties = ensure_object(target, "properties");
            target = properties
                .entry((*segment).to_owned())
                .or_insert_with(|| serde_json::json!({ "type": "object" }));
        }
        if let Some(properties) = entry.helper.get("properties").and_then(Value::as_object) {
            ensure_object(target, "properties").extend(
                properties
                    .iter()
                    .map(|(name, field)| (name.clone(), without_annotations(field))),
            );
        }
        let helper_required = string_list(entry.helper.get("required"));
        if !helper_required.is_empty()
            && let Some(map) = target.as_object_mut()
            && let Some(required) = map
                .entry("required")
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
        {
            for name in helper_required {
                let name = Value::String(name);
                if !required.contains(&name) {
                    required.push(name);
                }
            }
        }
    }
}

/// Proves `Valid(replacement) ⊆ Valid(conjunct)` for every conjunct.
fn check_included(
    replacement: &Value,
    conjuncts: &[Value],
    context: &Context,
) -> Result<(), String> {
    let subset = context.standalone(replacement, Some(&context.type_id));
    for conjunct in conjuncts {
        let superset = context.standalone(conjunct, context.parent_id.as_deref());
        let (verdict, diagnostics) =
            crate::schema_evolution::check_accepted_set_inclusion(&subset, &superset);
        if verdict.is_compatible() {
            continue;
        }
        let reasons: Vec<String> = diagnostics.iter().map(ToString::to_string).collect();
        let outcome = if verdict.is_unknown() {
            "is not proven to be included in"
        } else {
            "widens"
        };
        return Err(format!(
            "the override {outcome} the inherited declaration {conjunct}: {}{}",
            reasons.join("; "),
            null_hint(&subset, &superset)
        ));
    }
    Ok(())
}

/// Proves that the helper keeps `name` required wherever a level of the
/// ancestor requires it, by comparing objects that differ only in that.
fn check_requiredness(
    name: &str,
    helper_required: &[String],
    levels: &[Value],
    context: &Context,
) -> Result<(), String> {
    let level = |required: bool| {
        let required: Vec<&str> = if required { vec![name] } else { Vec::new() };
        context.standalone(
            &serde_json::json!({ "type": "object", "required": required }),
            None,
        )
    };
    let subset = level(helper_required.iter().any(|field| field == name));
    for inherited in levels {
        let superset = level(
            string_list(inherited.get("required"))
                .iter()
                .any(|f| f == name),
        );
        let (verdict, diagnostics) =
            crate::schema_evolution::check_accepted_set_inclusion(&subset, &superset);
        if !verdict.is_compatible() {
            let reasons: Vec<String> = diagnostics.iter().map(ToString::to_string).collect();
            return Err(format!(
                "is required by the ancestor, but the helper field is optional (an `Option<_>` \
                 or `#[serde(default)]` field); an override cannot make a field optional: {}",
                reasons.join("; ")
            ));
        }
    }
    Ok(())
}

/// Explains the common `Option<_>`-helper mistake.
fn null_hint(subset: &Value, superset: &Value) -> &'static str {
    let admits_null = |schema: &Value| {
        crate::json_schema::validator_for(schema).is_ok_and(|v| v.is_valid(&Value::Null))
    };
    if admits_null(subset) && !admits_null(superset) {
        "; the override admits null (an `Option<_>` field?) but the inherited field does not: \
         use a non-Option type"
    } else {
        ""
    }
}

/// Keywords that, on a level the path to a field passes through, could
/// constrain the field outside the subschemas collected for it.
const HIDING_KEYWORDS: &[&str] = &[
    "$ref",
    "anyOf",
    "oneOf",
    "not",
    "if",
    "then",
    "else",
    "patternProperties",
    "dependencies",
    "dependentSchemas",
];

/// The subschemas that apply to the property at `path` (generic slots, then
/// a field name).
struct Declarations {
    /// Each applicable subschema across all `allOf` branches, without its own
    /// `allOf` (whose branches are collected separately).
    conjuncts: Vec<Value>,
    /// Whether some level declares the property in `properties`, rather than
    /// only constraining it through `additionalProperties`.
    declared: bool,
}

/// Collects [`Declarations`]. Fails when a level on the way could constrain
/// the property in a way this walk does not follow, so that no conjunct is
/// silently missed.
fn declarations_at(schema: &Value, path: &[&str]) -> Result<Declarations, String> {
    let mut found = Declarations {
        conjuncts: Vec::new(),
        declared: false,
    };
    collect_declarations(schema, path, 0, true, &mut found)?;
    Ok(found)
}

fn collect_declarations(
    schema: &Value,
    path: &[&str],
    depth: usize,
    declared: bool,
    found: &mut Declarations,
) -> Result<(), String> {
    use crate::schema_evolution::MAX_RECURSION_DEPTH;
    if depth >= MAX_RECURSION_DEPTH {
        return Err(format!(
            "the inherited schema nests deeper than {MAX_RECURSION_DEPTH} levels on the way to \
             this field; it cannot be overridden"
        ));
    }
    if let Some(branches) = schema.get("allOf").and_then(Value::as_array) {
        for branch in branches {
            collect_declarations(branch, path, depth + 1, declared, found)?;
        }
    }
    let Some((head, rest)) = path.split_first() else {
        found.declared |= declared;
        match schema.as_object() {
            Some(map) => {
                let own: Map<String, Value> = map
                    .iter()
                    .filter(|(keyword, _)| *keyword != "allOf")
                    .map(|(keyword, value)| (keyword.clone(), value.clone()))
                    .collect();
                if !own.is_empty() || map.is_empty() {
                    found.conjuncts.push(Value::Object(own));
                }
            }
            None => found.conjuncts.push(schema.clone()),
        }
        return Ok(());
    };
    match schema {
        Value::Bool(true) => return Ok(()),
        Value::Bool(false) => {
            return Err(format!(
                "an inherited level on the way to '{head}' is `false`; it cannot be overridden"
            ));
        }
        _ => {}
    }
    if let Some(keyword) = HIDING_KEYWORDS
        .iter()
        .find(|keyword| schema.get(**keyword).is_some())
    {
        return Err(format!(
            "an inherited level on the way to '{head}' uses '{keyword}', which could constrain \
             the field outside its declarations; it cannot be overridden"
        ));
    }
    // A level's `additionalProperties` applies to the names it does not
    // declare itself.
    if let Some(property) = schema.get("properties").and_then(|p| p.get(*head)) {
        collect_declarations(property, rest, depth + 1, true, found)?;
    } else if let Some(additional) = schema.get("additionalProperties") {
        collect_declarations(additional, rest, depth + 1, false, found)?;
    }
    Ok(())
}

/// The helper's fields, after checking that its root is a plain object
/// schema whose fields can be read one by one.
fn helper_fields(helper: &Value) -> Result<&Map<String, Value>, String> {
    let root = helper
        .as_object()
        .ok_or_else(|| "helper schema is not an object schema".to_owned())?;
    if root.get("type") != Some(&Value::String("object".to_owned())) {
        return Err(format!(
            "helper must be a struct with named fields (its schema type is {}), not an enum, \
             map, newtype or `Option`",
            root.get("type").unwrap_or(&Value::Null)
        ));
    }
    if let Some(keyword) = root.keys().find(|keyword| {
        !HELPER_ROOT_KEYWORDS.contains(&keyword.as_str())
            && !ANNOTATIONS.contains(&keyword.as_str())
    }) {
        let reason = if keyword == "definitions" || keyword == "$defs" || keyword == "$ref" {
            "a recursive helper type cannot be inlined"
        } else {
            "it has no field-wise meaning"
        };
        return Err(format!(
            "helper root keyword '{keyword}' is not supported: {reason}"
        ));
    }
    // `false` is a partial view's own closedness and is not carried over; a
    // schema-valued `additionalProperties` (flattened map) constrains fields
    // the helper does not name.
    if root
        .get("additionalProperties")
        .is_some_and(|ap| !ap.is_boolean())
    {
        return Err(
            "helper root 'additionalProperties' schema (a flattened map?) is not supported"
                .to_owned(),
        );
    }
    if contains_ref(helper) {
        return Err(
            "helper is a recursive type (a `$ref` survived inlining); recursive overrides are \
             not supported"
                .to_owned(),
        );
    }
    match root.get("properties").and_then(Value::as_object) {
        Some(fields) if !fields.is_empty() => Ok(fields),
        _ => Err("helper declares no fields; it would override nothing".to_owned()),
    }
}

/// `schema` without annotation keywords at any schema position: helper
/// documentation and `#[serde(default)]` values are not part of the
/// replacement.
fn without_annotations(schema: &Value) -> Value {
    let mut schema = schema.clone();
    strip_annotations(&mut schema);
    schema
}

fn strip_annotations(schema: &mut Value) {
    if let Some(map) = schema.as_object_mut() {
        map.retain(|keyword, _| !ANNOTATIONS.contains(&keyword.as_str()));
    }
    for_each_subschema_mut(schema, &mut strip_annotations);
}

fn ensure_object<'v>(value: &'v mut Value, key: &str) -> &'v mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    let Value::Object(map) = value else {
        unreachable!("replaced by an object above")
    };
    let entry = map
        .entry(key.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    let Value::Object(entry) = entry else {
        unreachable!("replaced by an object above")
    };
    entry
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

const ANNOTATIONS: &[&str] = &[
    "title",
    "description",
    "default",
    "examples",
    "deprecated",
    "readOnly",
    "writeOnly",
    "$comment",
];

/// `true` if `ancestor` is a strict chain prefix of `descendant` (both type
/// IDs ending in `~`). Used by the macro's compile-time ancestor guard.
#[doc(hidden)]
#[must_use]
pub const fn is_strict_ancestor_type_id(ancestor: &str, descendant: &str) -> bool {
    let a = ancestor.as_bytes();
    let d = descendant.as_bytes();
    if a.is_empty() || a.len() >= d.len() || a[a.len() - 1] != b'~' {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != d[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Const string equality for the macro's duplicate-anchor guard.
#[doc(hidden)]
#[must_use]
pub const fn const_str_eq(left: &str, right: &str) -> bool {
    let l = left.as_bytes();
    let r = right.as_bytes();
    if l.len() != r.len() {
        return false;
    }
    let mut i = 0;
    while i < l.len() {
        if l[i] != r[i] {
            return false;
        }
        i += 1;
    }
    true
}

#[cfg(test)]
#[path = "schema_overrides_test.rs"]
mod schema_overrides_test;
