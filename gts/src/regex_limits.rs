//! Reports regex matches that the validator's engine cannot complete.
//!
//! `jsonschema` 0.57 drops `fancy-regex` runtime errors such as an exceeded
//! backtrack limit. `patternProperties`, and the `additionalProperties` and
//! `unevaluatedProperties` key classification, treat the property name as
//! unmatched, and `pattern` reports a plain mismatch, which `not` inverts into
//! success. GTS requires an explicit validation error instead (spec v0.14
//! §11.0, "Regular-expression semantics").
//!
//! TEMPORARY. The fix belongs upstream: an engine failure recorded in the
//! validation context and surfaced by `is_valid`, `validate`, `iter_errors`
//! and `evaluate`. Remove this module once `jsonschema` is bumped to a release
//! with that fix, `jsonschema_still_drops_regex_engine_errors` fails, and the
//! regression tests here and in `store_test.rs` pass without it.
//!
//! Until then, validation first replays the matches the validator can
//! perform, with the same documents, engine, limits and pattern translation,
//! and fails on the first one the engine cannot complete. References resolve
//! as the validator resolves them, keywords apply under the dialect in effect
//! where they appear, and a reference that does not resolve, like an exceeded
//! work budget, fails the check rather than skipping it.
//!
//! Where the replay cannot tell which subschemas the validator evaluates, it
//! enters all of them:
//!
//! - every `anyOf` branch past one that holds, and every `oneOf` branch past
//!   a second one that holds;
//! - every item for `contains`, past one that matches;
//! - both `then` and `else` under an `if` other than `true`, `false` or `{}`;
//! - every value and item for `unevaluatedProperties` and `unevaluatedItems`;
//! - every same-named `$dynamicAnchor` for a `$ref` or `$dynamicRef` whose
//!   target declares one, which `referencing` 0.57 resolves through the
//!   dynamic scope for both keywords.
//!
//! In those subschemas, a regex failure, an unresolved reference or the work
//! they add past the budget can reject an instance the validator would have
//! accepted. Each needs a pathological schema or instance; the replay never
//! accepts what an exact one would reject.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use referencing::{Draft, Registry, Resolver, ResourceRef, Uri};
use serde_json::{Map, Value};

/// The base URI `jsonschema` gives a schema without an `$id`.
const DEFAULT_BASE_URI: &str = "json-schema:///";

/// Subschema visits plus regex matches one check may perform.
///
/// Each costs at most one engine run, itself bounded by the engine's
/// backtrack limit, and the work still pending never exceeds what remains, so
/// this also bounds the memory a check holds.
pub const DEFAULT_STEP_BUDGET: usize = 1_000_000;

/// Why [`RegexGuard::check`] rejected an instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegexCheckError {
    /// The engine could not complete a match the validator performs.
    Exhausted(String),
    /// Whether the engine completes every match could not be established.
    Unchecked(String),
}

impl std::fmt::Display for RegexCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exhausted(message) => f.write_str(message),
            Self::Unchecked(reason) => {
                write!(
                    f,
                    "regular expression matches could not be checked: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for RegexCheckError {}

/// [`RegexGuard::check`] for a single instance.
///
/// # Errors
/// See [`RegexGuard::new`] and [`RegexGuard::check`].
pub fn exhausted_match(
    schema: &Value,
    resources: &[(String, &Value)],
    instance: &Value,
) -> Result<(), RegexCheckError> {
    RegexGuard::new(schema, resources)
        .map_err(RegexCheckError::Unchecked)?
        .check(instance)
}

/// The replay prepared for one schema, reusable across instances.
pub struct RegexGuard {
    /// `None` when no document holds a regex, so nothing can exhaust.
    prepared: Option<Prepared>,
    budget: usize,
}

/// The bases of the resources declaring each `$dynamicAnchor` name.
type DynamicAnchors = HashMap<String, Vec<Uri<String>>>;

struct Prepared {
    registry: Registry<'static>,
    root_uri: Uri<String>,
    root: Arc<Value>,
    /// Every pattern in the documents' schema positions, compiled as the
    /// validator compiles it; `None` for one the validator cannot compile.
    regexes: HashMap<String, Option<fancy_regex::Regex>>,
    /// Patterns a reference reaches outside those positions, compiled on
    /// first use and kept for every later check.
    late_regexes: Mutex<HashMap<String, Option<Arc<fancy_regex::Regex>>>>,
    /// How many patterns were compiled late.
    #[cfg(test)]
    late_compilations: std::sync::atomic::AtomicUsize,
    /// The resources declaring a `$dynamicAnchor`, by anchor name.
    dynamic_anchors: DynamicAnchors,
    /// Whether some resource declares `"$recursiveAnchor": true`.
    recursive_anchors: bool,
}

impl RegexGuard {
    /// Prepares the replay of `schema`.
    ///
    /// `resources` are the documents `schema` references, by URI, as given to
    /// the validator.
    ///
    /// # Errors
    /// Why the documents could not be prepared as the validator prepares them.
    pub fn new(schema: &Value, resources: &[(String, &Value)]) -> Result<Self, String> {
        let documents: Vec<&Value> = std::iter::once(schema)
            .chain(resources.iter().map(|(_, document)| *document))
            .collect();
        if !documents.iter().any(|document| mentions_regex(document)) {
            return Ok(Self {
                prepared: None,
                budget: DEFAULT_STEP_BUDGET,
            });
        }
        let regexes = collect_regexes(&documents);

        // As `jsonschema` 0.57 does (`compiler::resolve_base_uri`), the root is
        // registered under its resolved `$id` and then entered, which applies
        // a relative `$id` a second time. The replay must reach the targets the
        // validator reaches, so it repeats this rather than the specification.
        let root_resource = Draft::default().detect(schema).create_resource_ref(schema);
        let root_uri = referencing::uri::from_str(root_resource.id().unwrap_or(DEFAULT_BASE_URI))
            .map_err(|e| format!("the schema's base URI is invalid: {e}"))?;
        let root = Arc::new(schema.clone());
        let documents: Vec<(String, Arc<Value>)> = resources
            .iter()
            .map(|(uri, document)| (uri.clone(), Arc::new((*document).clone())))
            .chain(std::iter::once((
                root_uri.as_str().to_owned(),
                Arc::clone(&root),
            )))
            .collect();
        let registry = Registry::new()
            .extend(
                documents
                    .iter()
                    .map(|(uri, document)| (uri.as_str(), Arc::clone(document))),
            )
            .and_then(referencing::RegistryBuilder::prepare)
            .map_err(|e| format!("the referenced documents could not be registered: {e}"))?;
        let (dynamic_anchors, recursive_anchors) = scan_anchors(&registry, &documents)?;

        Ok(Self {
            prepared: Some(Prepared {
                registry,
                root_uri,
                root,
                regexes,
                late_regexes: Mutex::new(HashMap::new()),
                #[cfg(test)]
                late_compilations: std::sync::atomic::AtomicUsize::new(0),
                dynamic_anchors,
                recursive_anchors,
            }),
            budget: DEFAULT_STEP_BUDGET,
        })
    }

    /// This guard with a different step budget.
    #[cfg(test)]
    pub(crate) fn with_budget(mut self, budget: usize) -> Self {
        self.budget = budget;
        self
    }

    /// Replays every match the validator may perform on `instance`.
    ///
    /// # Errors
    /// [`RegexCheckError::Exhausted`] for the first match the engine could not
    /// complete, or [`RegexCheckError::Unchecked`] when a reference does not
    /// resolve or the step budget runs out.
    pub fn check(&self, instance: &Value) -> Result<(), RegexCheckError> {
        let Some(prepared) = &self.prepared else {
            return Ok(());
        };
        let mut replay = Replay::new(prepared, self.budget);
        // Entering the root from its registered URI, as the validator does.
        let start = replay.add_scope(
            prepared.registry.resolver(prepared.root_uri.clone()),
            Draft::default(),
        )?;
        replay.push_child(start, &prepared.root, [Subject::Value(instance)])?;
        replay.run()
    }
}

impl Prepared {
    /// The compiled `pattern` a reference reached outside the schema
    /// positions, compiled once for every check.
    fn late_regex(&self, pattern: &str) -> Option<Arc<fancy_regex::Regex>> {
        // The lock guards plain map updates, so a poisoned one is still sound.
        let mut late = self
            .late_regexes
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        late.entry(pattern.to_owned())
            .or_insert_with(|| {
                #[cfg(test)]
                self.late_compilations
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                compile(pattern).map(Arc::new)
            })
            .clone()
    }
}

/// Whether any object in `value` holds a regex keyword, wherever it sits: a
/// reference may name a subschema outside the positions its dialect defines.
fn mentions_regex(value: &Value) -> bool {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            Value::Object(map) => {
                if map.contains_key("pattern") || map.contains_key("patternProperties") {
                    return true;
                }
                pending.extend(map.values());
            }
            Value::Array(items) => pending.extend(items),
            _ => {}
        }
    }
    false
}

/// Every `pattern` and `patternProperties` key in the documents' schema
/// positions, compiled. Data such as `default` or `const` is never compiled;
/// a pattern a reference reaches outside these positions is compiled when it
/// is first matched.
fn collect_regexes(documents: &[&Value]) -> HashMap<String, Option<fancy_regex::Regex>> {
    let mut regexes = HashMap::new();
    let mut pending: Vec<(&Value, Draft)> = documents
        .iter()
        .map(|document| (*document, Draft::default().detect(document)))
        .collect();
    while let Some((node, draft)) = pending.pop() {
        if let Some(Value::String(pattern)) = node.get("pattern") {
            regexes
                .entry(pattern.clone())
                .or_insert_with(|| compile(pattern));
        }
        if let Some(Value::Object(patterns)) = node.get("patternProperties") {
            for pattern in patterns.keys() {
                regexes
                    .entry(pattern.clone())
                    .or_insert_with(|| compile(pattern));
            }
        }
        pending.extend(
            draft
                .subresources_of(node)
                .map(|child| (child, draft.detect(child))),
        );
    }
    regexes
}

/// Compiles `pattern` as `jsonschema` does under its default options.
fn compile(pattern: &str) -> Option<fancy_regex::Regex> {
    let translated = jsonschema_regex::to_rust_regex(pattern).ok()?;
    fancy_regex::Regex::new(&translated).ok()
}

/// The resources declaring each `$dynamicAnchor`, and whether any declares
/// `"$recursiveAnchor": true`.
///
/// Walks the schema positions the registry indexes anchors in, so a value
/// under `default`, `const`, `enum` or an unknown keyword is never a target.
fn scan_anchors(
    registry: &Registry<'_>,
    documents: &[(String, Arc<Value>)],
) -> Result<(DynamicAnchors, bool), String> {
    let mut dynamic = DynamicAnchors::new();
    let mut recursive = false;
    for (uri, document) in documents {
        let base = referencing::uri::from_str(uri.trim_end_matches('#'))
            .map_err(|e| format!("'{uri}' is not a valid URI: {e}"))?;
        // As the registry indexes them: a document's `$id` applies from the
        // URI it is registered under, a subschema's from its resource.
        let mut pending = vec![(
            document.as_ref(),
            Draft::default().detect(document),
            registry.resolver(base),
        )];
        while let Some((node, draft, resolver)) = pending.pop() {
            let resolver = resolver
                .in_subresource(ResourceRef::new(node, draft))
                .map_err(|e| format!("a subschema's '$id' is invalid: {e}"))?;
            if draft >= Draft::Draft202012
                && let Some(Value::String(name)) = node.get("$dynamicAnchor")
            {
                let bases = dynamic.entry(name.clone()).or_default();
                let base = (*resolver.base_uri()).clone();
                if !bases.contains(&base) {
                    bases.push(base);
                }
            }
            recursive |= draft == Draft::Draft201909 && has_recursive_anchor(node);
            for child in draft.subresources_of(node) {
                pending.push((child, draft.detect(child), resolver.clone()));
            }
        }
    }
    Ok((dynamic, recursive))
}

/// Whether `schema` is a `$recursiveRef` target the dynamic scope may extend.
fn has_recursive_anchor(schema: &Value) -> bool {
    schema.get("$recursiveAnchor").and_then(Value::as_bool) == Some(true)
}

/// What a subschema applies to.
#[derive(Clone, Copy)]
enum Subject<'i> {
    Value(&'i Value),
    /// A property name, which `propertyNames` validates as a string.
    Name(&'i String),
}

impl Subject<'_> {
    fn address(self) -> usize {
        match self {
            Self::Value(value) => std::ptr::from_ref(value) as usize,
            Self::Name(name) => std::ptr::from_ref(name) as usize,
        }
    }
}

/// Where a subschema sits: what its references resolve against, and the
/// dialect its keywords follow.
struct Scope<'g> {
    resolver: Resolver<'g>,
    draft: Draft,
}

/// What distinguishes one [`Scope`] from another for the replay.
///
/// Beyond the base URI and dialect, only `$recursiveRef` depends on the
/// dynamic scope (`$dynamicRef` targets are over-approximated): through
/// whether the scope is empty, which decides whether the next lookup extends
/// it, and the outermost resource of its leading run of recursive anchors,
/// which is where `Resolver::lookup_recursive_ref` lands. Both evolve from
/// this key alone, so equal keys replay alike, and a reference cycle reaches a
/// key it already had.
#[derive(Hash, PartialEq, Eq)]
struct ScopeKey {
    base: String,
    draft: Draft,
    recursion: Option<(bool, Option<String>)>,
}

/// A subschema to replay against a subject, within a scope.
#[derive(Clone, Copy)]
struct Task<'g, 'i> {
    scope: usize,
    schema: &'g Map<String, Value>,
    subject: Subject<'i>,
    /// Whether a reference led here; see [`Replay::visit`].
    via_reference: bool,
}

/// The keywords of one schema object the replay may act on.
#[derive(Default)]
struct Keywords<'g> {
    reference: Option<&'g Value>,
    dynamic_reference: Option<&'g Value>,
    recursive_reference: Option<&'g Value>,
    all_of: Option<&'g Value>,
    any_of: Option<&'g Value>,
    one_of: Option<&'g Value>,
    not: Option<&'g Value>,
    condition: Option<&'g Value>,
    then: Option<&'g Value>,
    otherwise: Option<&'g Value>,
    pattern: Option<&'g Value>,
    properties: Option<&'g Value>,
    pattern_properties: Option<&'g Value>,
    additional_properties: Option<&'g Value>,
    unevaluated_properties: Option<&'g Value>,
    property_names: Option<&'g Value>,
    dependencies: Option<&'g Value>,
    dependent_schemas: Option<&'g Value>,
    items: Option<&'g Value>,
    prefix_items: Option<&'g Value>,
    additional_items: Option<&'g Value>,
    unevaluated_items: Option<&'g Value>,
    contains: Option<&'g Value>,
}

/// The names [`Keywords`] holds that apply to any subject.
const GENERAL_KEYWORDS: [&str; 10] = [
    "$ref",
    "$dynamicRef",
    "$recursiveRef",
    "allOf",
    "anyOf",
    "oneOf",
    "not",
    "if",
    "then",
    "else",
];
const STRING_KEYWORDS: [&str; 1] = ["pattern"];
const OBJECT_KEYWORDS: [&str; 7] = [
    "properties",
    "patternProperties",
    "additionalProperties",
    "unevaluatedProperties",
    "propertyNames",
    "dependencies",
    "dependentSchemas",
];
const ARRAY_KEYWORDS: [&str; 5] = [
    "items",
    "prefixItems",
    "additionalItems",
    "unevaluatedItems",
    "contains",
];

impl<'g> Keywords<'g> {
    /// One pass over a narrow object's members is cheaper than a lookup per
    /// keyword; a wide one, holding many annotations, is searched by name for
    /// the keywords that can act on `subject`.
    fn of(node: &'g Map<String, Value>, subject: Subject<'_>) -> Self {
        let mut keywords = Self::default();
        let specific: &[&str] = match subject {
            Subject::Name(_) | Subject::Value(Value::String(_)) => &STRING_KEYWORDS,
            Subject::Value(Value::Object(_)) => &OBJECT_KEYWORDS,
            Subject::Value(Value::Array(_)) => &ARRAY_KEYWORDS,
            Subject::Value(_) => &[],
        };
        if node.len() <= GENERAL_KEYWORDS.len() + specific.len() {
            for (name, value) in node {
                if let Some(slot) = keywords.slot(name) {
                    *slot = Some(value);
                }
            }
        } else {
            for name in GENERAL_KEYWORDS.iter().chain(specific) {
                if let (Some(value), Some(slot)) = (node.get(*name), keywords.slot(name)) {
                    *slot = Some(value);
                }
            }
        }
        keywords
    }

    fn slot(&mut self, name: &str) -> Option<&mut Option<&'g Value>> {
        Some(match name {
            "$ref" => &mut self.reference,
            "$dynamicRef" => &mut self.dynamic_reference,
            "$recursiveRef" => &mut self.recursive_reference,
            "allOf" => &mut self.all_of,
            "anyOf" => &mut self.any_of,
            "oneOf" => &mut self.one_of,
            "not" => &mut self.not,
            "if" => &mut self.condition,
            "then" => &mut self.then,
            "else" => &mut self.otherwise,
            "pattern" => &mut self.pattern,
            "properties" => &mut self.properties,
            "patternProperties" => &mut self.pattern_properties,
            "additionalProperties" => &mut self.additional_properties,
            "unevaluatedProperties" => &mut self.unevaluated_properties,
            "propertyNames" => &mut self.property_names,
            "dependencies" => &mut self.dependencies,
            "dependentSchemas" => &mut self.dependent_schemas,
            "items" => &mut self.items,
            "prefixItems" => &mut self.prefix_items,
            "additionalItems" => &mut self.additional_items,
            "unevaluatedItems" => &mut self.unevaluated_items,
            "contains" => &mut self.contains,
            _ => return None,
        })
    }
}

struct Replay<'g, 'i> {
    prepared: &'g Prepared,
    scopes: Vec<Scope<'g>>,
    scope_ids: HashMap<ScopeKey, usize>,
    pending: Vec<Task<'g, 'i>>,
    /// Reference targets, subjects and scopes already replayed, which ends
    /// reference cycles.
    visited: HashSet<(usize, usize, usize)>,
    budget: usize,
    spent: usize,
}

impl<'g, 'i> Replay<'g, 'i> {
    fn new(prepared: &'g Prepared, budget: usize) -> Self {
        Self {
            prepared,
            scopes: Vec::new(),
            scope_ids: HashMap::new(),
            pending: Vec::new(),
            visited: HashSet::new(),
            budget,
            spent: 0,
        }
    }

    fn run(&mut self) -> Result<(), RegexCheckError> {
        while let Some(task) = self.pending.pop() {
            self.spend()?;
            self.visit(task)?;
        }
        Ok(())
    }

    /// Takes one step of the budget.
    fn spend(&mut self) -> Result<(), RegexCheckError> {
        if self.spent >= self.budget {
            return Err(RegexCheckError::Unchecked(format!(
                "the check needs more than {} steps",
                self.budget
            )));
        }
        self.spent += 1;
        Ok(())
    }

    /// Fails unless `steps` more fit in the budget beside the work pending.
    ///
    /// Each queued task, like each match, costs a step, so work past what the
    /// budget has left can only fail; refusing it before collecting or
    /// queueing it bounds the memory a check holds.
    fn ensure_room(&self, steps: usize) -> Result<(), RegexCheckError> {
        if self.pending.len().saturating_add(steps) > self.budget - self.spent {
            return Err(RegexCheckError::Unchecked(format!(
                "the check needs more than {} steps",
                self.budget
            )));
        }
        Ok(())
    }

    /// The scope a resolver and dialect make, shared with an equivalent one.
    fn add_scope(
        &mut self,
        resolver: Resolver<'g>,
        draft: Draft,
    ) -> Result<usize, RegexCheckError> {
        let recursion = if self.prepared.recursive_anchors {
            Some(self.recursion_state(&resolver)?)
        } else {
            None
        };
        let key = ScopeKey {
            base: resolver.base_uri().as_str().to_owned(),
            draft,
            recursion,
        };
        if let Some(&id) = self.scope_ids.get(&key) {
            return Ok(id);
        }
        self.scopes.push(Scope { resolver, draft });
        self.scope_ids.insert(key, self.scopes.len() - 1);
        Ok(self.scopes.len() - 1)
    }

    /// See [`ScopeKey`]: mirrors the walk of `Resolver::lookup_recursive_ref`.
    fn recursion_state(
        &mut self,
        resolver: &Resolver<'g>,
    ) -> Result<(bool, Option<String>), RegexCheckError> {
        let scopes = resolver.dynamic_scope();
        let mut outermost = None;
        for uri in &scopes {
            self.spend()?;
            match resolver.lookup(uri.as_str()) {
                Ok(entered) if has_recursive_anchor(entered.contents()) => {
                    outermost = Some(uri.as_str().to_owned());
                }
                _ => break,
            }
        }
        Ok((scopes.is_empty(), outermost))
    }

    /// The scope inside `child`, a subschema in place: its `$schema` and `$id`
    /// apply, as they do when the validator compiles it.
    fn enter(&mut self, scope: usize, child: &'g Value) -> Result<usize, RegexCheckError> {
        let parent = &self.scopes[scope];
        let draft = parent.draft.detect(child);
        let resource = ResourceRef::new(child, draft);
        if resource.id().is_none() && draft == parent.draft {
            return Ok(scope);
        }
        let resolver = parent.resolver.in_subresource(resource).map_err(|e| {
            RegexCheckError::Unchecked(format!("a subschema's '$id' is invalid: {e}"))
        })?;
        self.add_scope(resolver, draft)
    }

    /// Queues `schema`, already in `scope`, against each of `subjects`.
    fn push<I>(
        &mut self,
        scope: usize,
        schema: &'g Value,
        subjects: I,
        via_reference: bool,
    ) -> Result<(), RegexCheckError>
    where
        I: IntoIterator<Item = Subject<'i>>,
        I::IntoIter: ExactSizeIterator,
    {
        // A boolean schema matches nothing.
        let Value::Object(schema) = schema else {
            return Ok(());
        };
        let subjects = subjects.into_iter();
        self.ensure_room(subjects.len())?;
        self.pending.extend(subjects.map(|subject| Task {
            scope,
            schema,
            subject,
            via_reference,
        }));
        Ok(())
    }

    /// Queues `child`, a subschema in place under `scope`.
    fn push_child<I>(
        &mut self,
        scope: usize,
        child: &'g Value,
        subjects: I,
    ) -> Result<(), RegexCheckError>
    where
        I: IntoIterator<Item = Subject<'i>>,
        I::IntoIter: ExactSizeIterator,
    {
        if !child.is_object() {
            return Ok(());
        }
        let scope = self.enter(scope, child)?;
        self.push(scope, child, subjects, false)
    }

    /// Without references, the replay walks a tree: every subschema and
    /// subject pair is reached by one path, at most once. Only a reference
    /// can lead back to work already done, so only its targets are recorded.
    fn visit(&mut self, task: Task<'g, 'i>) -> Result<(), RegexCheckError> {
        let Task {
            scope,
            schema: node,
            subject,
            via_reference,
        } = task;
        if via_reference
            && !self
                .visited
                .insert((std::ptr::from_ref(node) as usize, subject.address(), scope))
        {
            return Ok(());
        }
        let draft = self.scopes[scope].draft;
        let keywords = Keywords::of(node, subject);

        if let Some(Value::String(reference)) = keywords.reference {
            self.follow_reference(scope, "$ref", reference, subject)?;
        }
        // Before 2019-09, `$ref` makes its siblings inert.
        if draft <= Draft::Draft7 && keywords.reference.is_some() {
            return Ok(());
        }
        if draft >= Draft::Draft202012
            && let Some(Value::String(reference)) = keywords.dynamic_reference
        {
            self.follow_reference(scope, "$dynamicRef", reference, subject)?;
        }
        if draft == Draft::Draft201909 && keywords.recursive_reference.is_some_and(Value::is_string)
        {
            let (contents, resolver, draft) = self.scopes[scope]
                .resolver
                .lookup_recursive_ref()
                .map_err(|e| {
                    RegexCheckError::Unchecked(format!("$recursiveRef does not resolve: {e}"))
                })?
                .into_inner();
            self.push_target(resolver, draft, contents, subject)?;
        }

        for list in [keywords.all_of, keywords.any_of, keywords.one_of] {
            if let Some(Value::Array(subschemas)) = list {
                for subschema in subschemas {
                    self.push_child(scope, subschema, [subject])?;
                }
            }
        }
        if let Some(subschema) = keywords.not {
            self.push_child(scope, subschema, [subject])?;
        }
        if draft >= Draft::Draft7 {
            self.visit_conditional(scope, &keywords, subject)?;
        }

        match subject {
            Subject::Name(text) | Subject::Value(Value::String(text)) => {
                if let Some(Value::String(pattern)) = keywords.pattern {
                    self.matches(pattern, text)?;
                }
                Ok(())
            }
            Subject::Value(Value::Object(object)) => {
                self.visit_object(scope, draft, &keywords, subject, object)
            }
            Subject::Value(Value::Array(items)) => self.visit_array(scope, draft, &keywords, items),
            Subject::Value(_) => Ok(()),
        }
    }

    /// Queues what `reference` names, and whatever the dynamic scope could
    /// select in its place.
    ///
    /// In 2020-12, `referencing` 0.57 resolves a plain-name fragment naming a
    /// `$dynamicAnchor` through the dynamic scope for `$ref` and `$dynamicRef`
    /// alike (`Anchor::Dynamic::resolve`). Which anchor that selects depends
    /// on how the validator got here, so every same-named one is queued,
    /// each entered as that resolution enters the one it selects: from the
    /// reference's own target URI, applying the anchor's `$id` if it has one.
    fn follow_reference(
        &mut self,
        scope: usize,
        keyword: &str,
        reference: &str,
        subject: Subject<'i>,
    ) -> Result<(), RegexCheckError> {
        // The validator skips an empty reference, which names the enclosing
        // resource.
        if reference.is_empty() {
            return Ok(());
        }
        let unresolved = |e: referencing::Error| {
            RegexCheckError::Unchecked(format!("{keyword} '{reference}' does not resolve: {e}"))
        };
        let (target, resolver, draft) = self.scopes[scope]
            .resolver
            .lookup(reference)
            .map_err(unresolved)?
            .into_inner();
        self.push_target(resolver, draft, target, subject)?;
        if self.scopes[scope].draft < Draft::Draft202012 {
            return Ok(());
        }
        let Some((uri, name)) = reference.rsplit_once('#') else {
            return Ok(());
        };
        if name.is_empty()
            || name.starts_with('/')
            || target.get("$dynamicAnchor").and_then(Value::as_str) != Some(name)
        {
            return Ok(());
        }
        // The resolver the selected anchor is entered from.
        let from = self.scopes[scope]
            .resolver
            .lookup(&format!("{uri}#"))
            .map_err(unresolved)?
            .resolver()
            .clone();
        let prepared = self.prepared;
        for declared in prepared.dynamic_anchors.get(name).into_iter().flatten() {
            let (contents, _, draft) = prepared
                .registry
                .resolver(declared.clone())
                .lookup(&format!("#{name}"))
                .map_err(unresolved)?
                .into_inner();
            let resolver = from
                .in_subresource(ResourceRef::new(contents, draft))
                .map_err(unresolved)?;
            self.push_target(resolver, draft, contents, subject)?;
        }
        Ok(())
    }

    /// Queues a reference target, in the scope its resolution entered.
    fn push_target(
        &mut self,
        resolver: Resolver<'g>,
        draft: Draft,
        target: &'g Value,
        subject: Subject<'i>,
    ) -> Result<(), RegexCheckError> {
        let scope = self.add_scope(resolver, draft)?;
        self.push(scope, target, [subject], true)
    }

    /// `then` applies only where `if` may hold, `else` only where it may fail.
    fn visit_conditional(
        &mut self,
        scope: usize,
        keywords: &Keywords<'g>,
        subject: Subject<'i>,
    ) -> Result<(), RegexCheckError> {
        let Some(condition) = keywords.condition else {
            return Ok(());
        };
        let (holds, fails) = match condition {
            Value::Bool(holds) => (*holds, !*holds),
            Value::Object(map) if map.is_empty() => (true, false),
            _ => {
                self.push_child(scope, condition, [subject])?;
                (true, true)
            }
        };
        for (branch, applies) in [(keywords.then, holds), (keywords.otherwise, fails)] {
            if applies && let Some(branch) = branch {
                self.push_child(scope, branch, [subject])?;
            }
        }
        Ok(())
    }

    fn visit_object(
        &mut self,
        scope: usize,
        draft: Draft,
        keywords: &Keywords<'g>,
        subject: Subject<'i>,
        object: &'i Map<String, Value>,
    ) -> Result<(), RegexCheckError> {
        // `dependentSchemas` replaced `dependencies` in 2019-09; GTS
        // validators ignore the older keyword there (see `json_schema`).
        for (dependents, since, until) in [
            (keywords.dependencies, Draft::Draft4, Draft::Draft7),
            (
                keywords.dependent_schemas,
                Draft::Draft201909,
                Draft::Draft202012,
            ),
        ] {
            if draft < since || draft > until {
                continue;
            }
            if let Some(Value::Object(dependents)) = dependents {
                for (name, subschema) in dependents {
                    if object.contains_key(name) {
                        self.push_child(scope, subschema, [subject])?;
                    }
                }
            }
        }

        let properties = keywords.properties.and_then(Value::as_object);
        if let Some(properties) = properties {
            for (name, subschema) in properties {
                if let Some(value) = object.get(name) {
                    self.push_child(scope, subschema, [Subject::Value(value)])?;
                }
            }
        }
        let additional = keywords.additional_properties;
        let patterns = keywords.pattern_properties.and_then(Value::as_object);
        // Classifying the names costs a step each, for a match or a queued
        // value, so an object too large to classify fails before anything is
        // collected for it.
        if additional.is_some() || patterns.is_some() {
            self.ensure_room(object.len())?;
        }
        // Which names some pattern matched, for `additionalProperties`.
        let mut matched = vec![
            false;
            if additional.is_some() {
                object.len()
            } else {
                0
            }
        ];
        if let Some(patterns) = patterns {
            for (pattern, subschema) in patterns {
                let mut values = Vec::new();
                for (index, (name, value)) in object.iter().enumerate() {
                    if self.matches(pattern, name)? {
                        values.push(Subject::Value(value));
                        if let Some(flag) = matched.get_mut(index) {
                            *flag = true;
                        }
                    }
                }
                self.push_child(scope, subschema, values)?;
            }
        }
        if let Some(subschema) = additional {
            let values: Vec<_> = object
                .iter()
                .zip(&matched)
                .filter(|((name, _), matched)| {
                    !**matched
                        && !properties.is_some_and(|properties| properties.contains_key(*name))
                })
                .map(|((_, value), _)| Subject::Value(value))
                .collect();
            self.push_child(scope, subschema, values)?;
        }
        if draft >= Draft::Draft201909
            && let Some(subschema) = keywords.unevaluated_properties
        {
            self.push_child(scope, subschema, object.values().map(Subject::Value))?;
        }
        if draft >= Draft::Draft6
            && let Some(subschema) = keywords.property_names
        {
            self.push_child(scope, subschema, object.keys().map(Subject::Name))?;
        }
        Ok(())
    }

    fn visit_array(
        &mut self,
        scope: usize,
        draft: Draft,
        keywords: &Keywords<'g>,
        items: &'i [Value],
    ) -> Result<(), RegexCheckError> {
        // A tuple, then the schema for the items past it: `prefixItems` and
        // `items` from 2020-12, an `items` array and `additionalItems` before,
        // where an `items` schema covers every item.
        let (tuple, rest) = if draft >= Draft::Draft202012 {
            (
                keywords.prefix_items.and_then(Value::as_array),
                keywords.items,
            )
        } else {
            match keywords.items {
                Some(Value::Array(tuple)) => (Some(tuple), keywords.additional_items),
                other => (None, other),
            }
        };
        let tuple = tuple.map_or(&[][..], Vec::as_slice);
        for (subschema, item) in tuple.iter().zip(items) {
            self.push_child(scope, subschema, [Subject::Value(item)])?;
        }
        if let Some(subschema) = rest {
            let tail = items.get(tuple.len()..).unwrap_or_default();
            self.push_child(scope, subschema, tail.iter().map(Subject::Value))?;
        }
        if draft >= Draft::Draft201909
            && let Some(subschema) = keywords.unevaluated_items
        {
            self.push_child(scope, subschema, items.iter().map(Subject::Value))?;
        }
        if draft >= Draft::Draft6
            && let Some(subschema) = keywords.contains
        {
            self.push_child(scope, subschema, items.iter().map(Subject::Value))?;
        }
        Ok(())
    }

    /// Whether `pattern` matches `text`, or the error the engine stopped with.
    fn matches(&mut self, pattern: &str, text: &str) -> Result<bool, RegexCheckError> {
        self.spend()?;
        let late;
        let regex = if let Some(regex) = self.prepared.regexes.get(pattern) {
            regex.as_ref()
        } else {
            late = self.prepared.late_regex(pattern);
            late.as_deref()
        };
        let Some(regex) = regex else {
            return Err(RegexCheckError::Unchecked(format!(
                "regular expression '{pattern}' does not compile"
            )));
        };
        // As `jsonschema` does, an engine panic is recovered as a failed match
        // instead of unwinding through the caller, which may hold the store.
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| regex.is_match(text))) {
            Ok(Ok(matched)) => Ok(matched),
            Ok(Err(error)) => Err(RegexCheckError::Exhausted(format!(
                "regular expression '{pattern}' could not be matched: {error}"
            ))),
            Err(_) => Err(RegexCheckError::Exhausted(format!(
                "regular expression '{pattern}' could not be matched: the engine panicked"
            ))),
        }
    }
}

#[cfg(test)]
#[path = "regex_limits_test.rs"]
mod regex_limits_test;
