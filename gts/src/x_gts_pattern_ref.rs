//! `x-gts-pattern-ref`: string fields holding GTS identifier patterns
//! (spec v0.16 §9.6.2, §10; ADR-0007).
//!
//! A value must be a GTS identifier pattern whose selection `M(value)` lies
//! within the selection of the operand, `M(operand)`. `M(p)` ranges over every
//! canonical GTS identifier, registered or not, so coverage is decided from
//! the two spellings alone and never consults the registry.
//!
//! Patterns are parsed here rather than through [`crate::GtsIdPattern`]: the
//! keyword admits no normalization (that parser trims), and version numbers
//! carry no bound beyond the §2.3 grammar (that parser stores `u32`).

use serde_json::{Map, Value};

use crate::gts::{GTS_ID_MAX_LENGTH, GTS_ID_PREFIX, GTS_ID_URI_PREFIX};

/// The keyword name.
pub const X_GTS_PATTERN_REF: &str = "x-gts-pattern-ref";

/// The selected-type self-reference, the one pointer operand allowed.
const SELF_ID_POINTER: &str = "/$id";

/// One complete `vendor.package.namespace.type.vMAJOR[.MINOR]` segment.
///
/// Numbers keep their canonical spelling, so equality is textual.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Segment {
    names: [String; 4],
    major: String,
    minor: Option<String>,
}

impl Segment {
    fn parse(text: &str) -> Option<Self> {
        let tokens: Vec<&str> = text.split('.').collect();
        if !(5..=6).contains(&tokens.len()) {
            return None;
        }
        let names = names_of(&tokens[..4])?;
        let major = tokens[4].strip_prefix('v').filter(|n| is_number(n))?;
        let minor = match tokens.get(5) {
            Some(minor) if is_number(minor) => Some((*minor).to_owned()),
            Some(_) => return None,
            None => None,
        };
        Some(Self {
            names: names.try_into().ok()?,
            major: major.to_owned(),
            minor,
        })
    }

    /// Whether `self`, read as a type segment of a pattern, admits every
    /// segment `other` admits: an omitted minor version accepts any minor,
    /// an explicit one only itself (§10).
    fn covers(&self, other: &Self) -> bool {
        self.names == other.names
            && self.major == other.major
            && (self.minor.is_none() || self.minor == other.minor)
    }

    /// The segment's length as spelled: four names, four dots, `v`, major
    /// and an optional `.minor`.
    fn spelled_len(&self) -> usize {
        self.names.iter().map(String::len).sum::<usize>()
            + 5
            + self.major.len()
            + self.minor.as_ref().map_or(0, |minor| 1 + minor.len())
    }
}

/// The part of a segment a wildcard follows: one to four name tokens and, after
/// all four, optionally the major version (`type.v1.*`). The bare version
/// marker (`type.v*`) adds nothing to `type.*`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Partial {
    names: Vec<String>,
    major: Option<String>,
}

impl Partial {
    /// Parses the text between the last `~` (or `gts.`) and the `*`.
    fn parse(text: &str) -> Option<Self> {
        if let Some(names) = text.strip_suffix(".v") {
            let names = names_of(&names.split('.').collect::<Vec<_>>())?;
            return (names.len() == 4).then_some(Self { names, major: None });
        }
        let tokens: Vec<&str> = text.strip_suffix('.')?.split('.').collect();
        match tokens.len() {
            1..=4 => Some(Self {
                names: names_of(&tokens)?,
                major: None,
            }),
            5 => Some(Self {
                names: names_of(&tokens[..4])?,
                major: Some(
                    tokens[4]
                        .strip_prefix('v')
                        .filter(|n| is_number(n))?
                        .to_owned(),
                ),
            }),
            _ => None,
        }
    }

    fn admits(&self, segment: &Segment) -> bool {
        segment.names.starts_with(&self.names)
            && self
                .major
                .as_ref()
                .is_none_or(|major| *major == segment.major)
    }

    fn covers(&self, other: &Self) -> bool {
        other.names.starts_with(&self.names)
            && self
                .major
                .as_ref()
                .is_none_or(|major| other.names.len() == 4 && other.major.as_ref() == Some(major))
    }

    /// The length of the shortest complete segment it admits: one-character
    /// names where they are missing and major `0` when none is given.
    fn shortest_completion(&self) -> usize {
        self.names.iter().map(String::len).sum::<usize>()
            + (4 - self.names.len())
            + 5
            + self.major.as_ref().map_or(1, String::len)
    }
}

/// The element ending a concrete instance identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Tail {
    Instance(Segment),
    Uuid,
}

/// The set `M(p)` a valid pattern selects (§10).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Selection {
    /// A concrete Instance ID: that identifier only.
    Exact {
        spelling: String,
        types: Vec<Segment>,
        tail: Tail,
    },
    /// Identifiers whose leading type segments are covered by `types`, then -
    /// when `next` is given - a further segment it admits, then anything. A
    /// concrete Type ID is the case without `next` (`root~` ≡ `root~*`).
    Prefixed {
        types: Vec<Segment>,
        next: Option<Partial>,
    },
}

/// A validated GTS identifier pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GtsPattern(Selection);

impl GtsPattern {
    /// Parses `text` under the §10 grammar, exactly as spelled.
    ///
    /// # Errors
    /// Why `text` is not a GTS identifier pattern.
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        if text.len() > GTS_ID_MAX_LENGTH {
            return Err(format!(
                "is longer than {GTS_ID_MAX_LENGTH} characters ({})",
                text.len()
            ));
        }
        let Some(body) = text.strip_prefix(GTS_ID_PREFIX) else {
            return Err(format!("must start with '{GTS_ID_PREFIX}'"));
        };
        Self::parse_body(text, body).ok_or_else(|| {
            "is neither a concrete GTS identifier nor a wildcard pattern ending in a \
             single '*' at a token boundary"
                .to_owned()
        })
    }

    fn parse_body(text: &str, body: &str) -> Option<Self> {
        if let Some(prefix) = body.strip_suffix('*') {
            if prefix.contains('*') {
                return None;
            }
            let mut parts: Vec<&str> = prefix.split('~').collect();
            let last = parts.pop()?;
            let types = parts
                .into_iter()
                .map(Segment::parse)
                .collect::<Option<Vec<_>>>()?;
            let next = if last.is_empty() {
                None
            } else {
                Some(Partial::parse(last)?)
            };
            return Some(Self(Selection::Prefixed { types, next }));
        }

        let mut parts: Vec<&str> = body.split('~').collect();
        let last = parts.pop()?;
        let types = parts
            .into_iter()
            .map(Segment::parse)
            .collect::<Option<Vec<_>>>()?;
        if last.is_empty() {
            return (!types.is_empty()).then_some(Self(Selection::Prefixed { types, next: None }));
        }
        // Instance IDs carry at least one type segment (§2.1).
        if types.is_empty() {
            return None;
        }
        let tail = if is_uuid(last) {
            Tail::Uuid
        } else {
            Tail::Instance(Segment::parse(last)?)
        };
        Some(Self(Selection::Exact {
            spelling: text.to_owned(),
            types,
            tail,
        }))
    }

    /// Whether this is a concrete GTS Type ID.
    fn is_type_id(&self, spelling: &str) -> bool {
        matches!(self.0, Selection::Prefixed { next: None, .. }) && spelling.ends_with('~')
    }

    /// The length of the shortest canonical identifier in `M(self)`.
    fn shortest_selected(&self) -> usize {
        match &self.0 {
            Selection::Exact { spelling, .. } => spelling.len(),
            Selection::Prefixed { types, next } => {
                let chain =
                    GTS_ID_PREFIX.len() + types.iter().map(|t| t.spelled_len() + 1).sum::<usize>();
                match next {
                    // A concrete Type ID selects itself.
                    None if !types.is_empty() => chain,
                    // A first segment is a type and ends in `~`; a later one
                    // may end an instance identifier.
                    next => {
                        let segment = next.as_ref().map_or_else(
                            || {
                                Partial {
                                    names: Vec::new(),
                                    major: None,
                                }
                                .shortest_completion()
                            },
                            Partial::shortest_completion,
                        );
                        chain + segment + usize::from(types.is_empty())
                    }
                }
            }
        }
    }

    /// Whether `M(other) ⊆ M(self)`.
    ///
    /// Identifiers have at most [`GTS_ID_MAX_LENGTH`] characters (§2.1), so a
    /// valid pattern whose every completion is longer selects nothing and is
    /// covered by any operand. Otherwise the bound changes no verdict: where
    /// the unbounded selections differ, a counterexample is found among the
    /// shortest identifiers `other` selects, respelling at most one name
    /// character or digit, at the same length.
    pub(crate) fn covers(&self, other: &Self) -> bool {
        other.shortest_selected() > GTS_ID_MAX_LENGTH || self.covers_unbounded(other)
    }

    /// Whether `M(other) ⊆ M(self)` over identifiers of any length.
    fn covers_unbounded(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Selection::Exact { spelling: a, .. }, Selection::Exact { spelling: b, .. }) => a == b,
            (Selection::Exact { .. }, Selection::Prefixed { .. }) => false,
            (
                Selection::Prefixed { types, next },
                Selection::Exact {
                    types: ids, tail, ..
                },
            ) => selects_instance(types, next.as_ref(), ids, tail),
            (
                Selection::Prefixed {
                    types: a,
                    next: a_next,
                },
                Selection::Prefixed {
                    types: b,
                    next: b_next,
                },
            ) => prefixed_covers(a, a_next.as_ref(), b, b_next.as_ref()),
        }
    }
}

/// Whether the prefixed selection `(types, next)` contains a concrete
/// instance identifier.
fn selects_instance(
    types: &[Segment],
    next: Option<&Partial>,
    ids: &[Segment],
    tail: &Tail,
) -> bool {
    if ids.len() < types.len() || !types.iter().zip(ids).all(|(a, b)| a.covers(b)) {
        return false;
    }
    let Some(next) = next else {
        return true;
    };
    match ids.get(types.len()) {
        Some(segment) => next.admits(segment),
        None => matches!(tail, Tail::Instance(segment) if next.admits(segment)),
    }
}

/// Coverage between two prefixed selections.
fn prefixed_covers(
    a: &[Segment],
    a_next: Option<&Partial>,
    b: &[Segment],
    b_next: Option<&Partial>,
) -> bool {
    if a.len() > b.len() {
        // Only one shape of `b` fixes fewer type segments and still fits: the
        // first-segment `type.v1.*`, which equals `type.v1~` because a
        // single-segment identifier is always a type (§10).
        return match (a, a_next, b, b_next) {
            ([only], None, [], Some(partial)) => {
                only.minor.is_none()
                    && partial.names.as_slice() == only.names.as_slice()
                    && partial.major.as_ref() == Some(&only.major)
            }
            _ => false,
        };
    }
    if !a.iter().zip(b).all(|(a, b)| a.covers(b)) {
        return false;
    }
    let Some(a_next) = a_next else {
        return true;
    };
    match b.get(a.len()) {
        Some(segment) => a_next.admits(segment),
        // `b` also selects identifiers that end before the segment `a`
        // requires, unless it constrains that segment itself.
        None => b_next.is_some_and(|b_next| a_next.covers(b_next)),
    }
}

fn names_of(tokens: &[&str]) -> Option<Vec<String>> {
    tokens
        .iter()
        .map(|token| is_name(token).then(|| (*token).to_owned()))
        .collect()
}

/// `[a-z_][a-z0-9_]*` (§2.1).
fn is_name(token: &str) -> bool {
    let mut bytes = token.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first == b'_')
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// `0 | [1-9][0-9]*` with no upper bound (§2.3).
fn is_number(text: &str) -> bool {
    !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'))
}

/// Lowercase hyphenated UUID (§2.3).
fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_digit() || (b'a'..=b'f').contains(&b),
        })
}

/// The pattern an operand denotes, or why the schema is invalid.
///
/// `selected` is the effective leaf type `/$id` names, without `gts://`.
pub fn resolve_operand(declared: &Value, selected: Option<&str>) -> Result<GtsPattern, String> {
    let Some(declared) = declared.as_str() else {
        return Err(format!(
            "{X_GTS_PATTERN_REF} operand must be a string, got {declared}"
        ));
    };
    if declared == SELF_ID_POINTER {
        let Some(selected) = selected else {
            return Err(format!(
                "{X_GTS_PATTERN_REF} operand '{SELF_ID_POINTER}' has no selected GTS Type \
                 Schema '$id' to resolve to"
            ));
        };
        return GtsPattern::parse(selected)
            .ok()
            .filter(|pattern| pattern.is_type_id(selected))
            .ok_or_else(|| {
                format!(
                    "{X_GTS_PATTERN_REF} operand '{SELF_ID_POINTER}' resolves to '{selected}', \
                     which is not a GTS Type Identifier"
                )
            });
    }
    GtsPattern::parse(declared).map_err(|reason| {
        format!(
            "{X_GTS_PATTERN_REF} operand '{declared}' {reason}; expected a GTS identifier, a \
             wildcard pattern or '{SELF_ID_POINTER}'"
        )
    })
}

/// The `/$id` a document names when it is the type being validated.
pub fn self_id(schema: &Value) -> Option<String> {
    schema
        .get("$id")
        .and_then(Value::as_str)
        .map(|id| id.strip_prefix(GTS_ID_URI_PREFIX).unwrap_or(id).to_owned())
}

/// Checks every operand at a schema position of `schema`, including its trait
/// schema, with `/$id` naming the document's own `$id`.
///
/// # Errors
/// The first unusable operand, with its location.
pub fn check_operands(schema: &Value) -> Result<(), String> {
    let selected = self_id(schema);
    let mut first = None;
    crate::schema_modifiers::for_each_schema_node(schema, &mut |node, location| {
        if first.is_some() {
            return;
        }
        if let Some(declared) = node.get(X_GTS_PATTERN_REF)
            && let Err(reason) = resolve_operand(declared, selected.as_deref())
        {
            let location = if location.is_empty() {
                X_GTS_PATTERN_REF.to_owned()
            } else {
                format!("{location}/{X_GTS_PATTERN_REF}")
            };
            first = Some(format!("{location}: {reason}"));
        }
    });
    first.map_or(Ok(()), Err)
}

/// Registers the keyword. Unusable operands fail compilation, so `not` cannot
/// invert them; `/$id` names `selected` wherever it is declared.
pub fn with_x_gts_pattern_ref(
    options: jsonschema::ValidationOptions,
    selected: Option<String>,
) -> jsonschema::ValidationOptions {
    options.with_keyword(X_GTS_PATTERN_REF, move |_parent, declared, _location| {
        let operand = resolve_operand(declared, selected.as_deref())
            .map_err(jsonschema::ValidationError::schema)?;
        let spelling = match declared.as_str() {
            Some(SELF_ID_POINTER) | None => selected.clone().unwrap_or_default(),
            Some(spelling) => spelling.to_owned(),
        };
        Ok(Box::new(PatternRefKeyword { operand, spelling }))
    })
}

/// One compiled declaration.
struct PatternRefKeyword {
    operand: GtsPattern,
    /// The operand as reported, `/$id` resolved.
    spelling: String,
}

impl PatternRefKeyword {
    /// Why a string fails; other JSON types are not constrained.
    fn violation(&self, instance: &Value) -> Option<String> {
        let value = instance.as_str()?;
        match GtsPattern::parse(value) {
            Err(reason) => Some(format!(
                "'{value}' is not a valid GTS identifier pattern: it {reason}"
            )),
            Ok(pattern) if !self.operand.covers(&pattern) => Some(format!(
                "'{value}' selects GTS identifiers outside '{}'",
                self.spelling
            )),
            Ok(_) => None,
        }
    }
}

impl<'i> jsonschema::Keyword<'i> for PatternRefKeyword {
    fn validate(&self, instance: &'i Value) -> Result<(), jsonschema::ValidationError<'i>> {
        match self.violation(instance) {
            Some(reason) => Err(jsonschema::ValidationError::custom(reason)),
            None => Ok(()),
        }
    }

    fn is_valid(&self, instance: &Value) -> bool {
        self.violation(instance).is_none()
    }
}

/// `schema` with every `/$id` operand replaced by the document's own `$id`, so
/// that two effective schemas can be compared by their literal operands. The
/// document is returned unchanged when it has no `$id` to substitute.
pub fn with_resolved_self_operands(schema: &Value) -> std::borrow::Cow<'_, Value> {
    match self_id(schema) {
        Some(selected) => with_self_operands_naming(schema, &selected),
        None => std::borrow::Cow::Borrowed(schema),
    }
}

/// `schema` with every `/$id` operand replaced by `selected`, for a fragment
/// validated on behalf of a type it does not name itself, such as an effective
/// trait schema (README §9.7.5).
pub fn with_self_operands_naming<'s>(
    schema: &'s Value,
    selected: &str,
) -> std::borrow::Cow<'s, Value> {
    let mut holders = std::collections::HashSet::new();
    crate::schema_modifiers::for_each_schema_node(schema, &mut |node, _| {
        if node.get(X_GTS_PATTERN_REF).and_then(Value::as_str) == Some(SELF_ID_POINTER) {
            holders.insert(std::ptr::from_ref(node));
        }
    });
    if holders.is_empty() {
        return std::borrow::Cow::Borrowed(schema);
    }
    std::borrow::Cow::Owned(substitute(schema, &holders, selected))
}

fn substitute(
    value: &Value,
    holders: &std::collections::HashSet<*const Map<String, Value>>,
    selected: &str,
) -> Value {
    match value {
        Value::Object(map) => {
            let mut copy: Map<String, Value> = map
                .iter()
                .map(|(key, child)| (key.clone(), substitute(child, holders, selected)))
                .collect();
            if holders.contains(&std::ptr::from_ref(map)) {
                copy.insert(
                    X_GTS_PATTERN_REF.to_owned(),
                    Value::String(selected.to_owned()),
                );
            }
            Value::Object(copy)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|child| substitute(child, holders, selected))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The literal pattern an already `/$id`-resolved operand denotes.
pub fn literal_operand(declared: &Value) -> Option<GtsPattern> {
    declared
        .as_str()
        .and_then(|text| GtsPattern::parse(text).ok())
}

#[cfg(test)]
#[path = "x_gts_pattern_ref_test.rs"]
mod x_gts_pattern_ref_test;
