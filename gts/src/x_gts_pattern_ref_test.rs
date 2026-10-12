use super::*;
use crate::schema_evolution::{
    CompatibilityVerdict, check_backward_compatibility, check_forward_compatibility,
};
use serde_json::json;

const RESOURCE: &str = "gts.x.core.resources.resource";

fn pattern(text: &str) -> GtsPattern {
    GtsPattern::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

fn covers(operand: &str, value: &str) -> bool {
    pattern(operand).covers(&pattern(value))
}

#[test]
fn grammar_accepts_section_10_patterns() {
    let longest = format!("gts.{}.*", "a".repeat(1018));
    for valid in [
        "gts.*",
        "gts.x.*",
        "gts.x.core.*",
        "gts.x.core.events.*",
        "gts.x.core.events.type.*",
        "gts.x.core.events.type.v*",
        "gts.x.core.events.type.v1.*",
        "gts.x.core.events.type.v0.*",
        "gts.x.core.events.type.v1~*",
        "gts.x.core.events.type.v1.0~*",
        "gts.x.core.events.type.v1~xyz.*",
        "gts.x.core.events.type.v1~x.y._.w.v1.*",
        "gts.x.core.events.type.v1~x.core._.audit_event.v1~xyz.app.*",
        "gts.x.core.events.type.v1~",
        "gts.x.core.events.type.v1.2~",
        "gts.x.core.events.topic.v1~ven.app._.custom_event_topic.v1.2",
        "gts.x.core.events.type.v1~7a1d2f34-5678-49ab-9012-abcdef123456",
        "gts.x.core.events.type.v99999999999999999999~",
        &longest,
    ] {
        assert!(GtsPattern::parse(valid).is_ok(), "{valid}");
    }
}

#[test]
fn grammar_rejects_everything_else_without_normalizing() {
    let too_long = format!("gts.{}.*", "a".repeat(1019));
    for invalid in [
        "",
        "*",
        "gts",
        "gts.",
        "gts*",
        "gts.vendor*",
        "gts.x.v*",
        "gts.x.core.events.type.v1*",
        "gts.x.core.events.type.v1.0*",
        "gts.x.core.events.type.v1.0.*",
        "gts.x.core.events.type.v.*",
        "gts.x.core.events.type.v1~abc*",
        "gts.x.core.events.type.vx.*",
        "gts.x.*.events.type.v1~",
        "gts.x.core.*~*",
        "gts.x.core.events.type.v1~**",
        "gts.x.core.events.type.v1~*~",
        "gts.x.core.events.type.v1~x.y._.w.v1@name",
        "gts.x.core.events.type.v1~550e8400-*",
        "gts.x.core.events.type.v1~7A1D2F34-5678-49AB-9012-ABCDEF123456",
        "GTS.x.core.*",
        "gts.X.core.*",
        " gts.x.core.*",
        "gts.x.core.* ",
        "gts.x.core.events.type.v1~\n",
        "gts://gts.x.core.events.type.v1~",
        "gts.x.core.events.type.v1",
        "gts.x.core.events.type.v01~",
        "gts.x.core.events.type.v1.01~*",
        "gts.x.core.events.type~",
        "gts.x.core.events.type.v1~~",
        "gts.x.core-1.events.*",
        "gts.1x.core.*",
        "gts.x.\u{441}ore.*",
        &too_long,
    ] {
        assert!(GtsPattern::parse(invalid).is_err(), "{invalid:?}");
    }
}

#[test]
fn coverage_follows_the_readme_tables() {
    let bucket = format!("{RESOURCE}.v1~acme.storage.resources.bucket");
    let uploads = format!("{RESOURCE}.v1~acme.prod._.uploads.v1");
    let cases = [
        (format!("{RESOURCE}.*"), format!("{RESOURCE}.v1~"), true),
        (format!("{RESOURCE}.*"), format!("{RESOURCE}.*"), true),
        (format!("{RESOURCE}.*"), format!("{bucket}.*"), true),
        (
            format!("{RESOURCE}.*"),
            format!("{bucket}.v1~acme.prod._.uploads.v1"),
            true,
        ),
        (
            format!("{RESOURCE}.*"),
            "gts.x.core.resources.*".to_owned(),
            false,
        ),
        (
            format!("{RESOURCE}.*"),
            "gts.x.core.events.event.*".to_owned(),
            false,
        ),
        (format!("{RESOURCE}.v1~"), format!("{RESOURCE}.*"), false),
        (format!("{RESOURCE}.v1~"), format!("{bucket}.*"), true),
        (
            format!("{RESOURCE}.v1.0~"),
            format!("{RESOURCE}.v1~"),
            false,
        ),
        (format!("{RESOURCE}.v1~"), format!("{RESOURCE}.v1.0~"), true),
        (uploads.clone(), uploads.clone(), true),
        (
            uploads.clone(),
            format!("{RESOURCE}.v1~acme.prod._.uploads.*"),
            false,
        ),
        (
            uploads.clone(),
            format!("{RESOURCE}.v1~acme.prod._.uploads.v1.0"),
            false,
        ),
        (
            format!("{RESOURCE}.v1~acme.prod._.uploads.v1.*"),
            uploads.clone(),
            true,
        ),
        (
            format!("{RESOURCE}.v1~acme.prod._.uploads.v1~*"),
            uploads.clone(),
            false,
        ),
        (
            format!("{RESOURCE}.v1~acme.prod._.uploads.v1~*"),
            format!("{RESOURCE}.v1~acme.prod._.uploads.v1.3~"),
            true,
        ),
        // First-segment `v1.*` is `v1~`.
        (format!("{RESOURCE}.v1.*"), format!("{RESOURCE}.v1~"), true),
        (format!("{RESOURCE}.v1~"), format!("{RESOURCE}.v1.*"), true),
        (
            format!("{RESOURCE}.v1.0~"),
            format!("{RESOURCE}.v1.*"),
            false,
        ),
        (format!("{RESOURCE}.v*"), format!("{RESOURCE}.*"), true),
        (
            format!("{RESOURCE}.v1~acme.*"),
            format!("{RESOURCE}.v1~"),
            false,
        ),
        (format!("{RESOURCE}.v1~"), format!("{RESOURCE}.v1~*"), true),
        (format!("{RESOURCE}.v1~*"), format!("{RESOURCE}.v1~"), true),
        (
            format!("{RESOURCE}.v1~"),
            format!("{RESOURCE}.v1~7a1d2f34-5678-49ab-9012-abcdef123456"),
            true,
        ),
        (
            format!("{RESOURCE}.v1~acme.*"),
            format!("{RESOURCE}.v1~7a1d2f34-5678-49ab-9012-abcdef123456"),
            false,
        ),
        // A textual prefix is not a segment boundary.
        (format!("{RESOURCE}.v1~"), format!("{RESOURCE}.v12~"), false),
        (
            format!("{RESOURCE}.v1~acme.prod._.uploads.v1"),
            format!("{RESOURCE}.v1~acme.prod._.uploads.v12"),
            false,
        ),
        ("gts.*".to_owned(), uploads, true),
        ("gts.*".to_owned(), "gts.*".to_owned(), true),
        (format!("{RESOURCE}.*"), "gts.*".to_owned(), false),
    ];
    for (operand, value, expected) in cases {
        assert_eq!(
            covers(&operand, &value),
            expected,
            "{operand} covers {value}"
        );
    }
}

/// `(prefix, wildcard, completion)`: around a padded name, a wildcard ending
/// and the ending of the shortest identifier it selects.
fn boundary_shapes() -> Vec<(String, &'static str, &'static str)> {
    let bucket = format!("{RESOURCE}.v1~acme.storage.resources.bucket.v2.3~");
    vec![
        // A first segment is a type: the completion ends in `~`.
        ("gts.".to_owned(), ".*", ".a._.a.v0~"),
        ("gts.".to_owned(), ".b.c.d.v*", ".b.c.d.v0~"),
        ("gts.x.".to_owned(), ".*", ".a.a.v0~"),
        // A later segment may end an instance identifier, one character
        // shorter; explicit minors and long majors count as spelled.
        (format!("{RESOURCE}.v1~"), ".*", ".a._.a.v0"),
        (format!("{RESOURCE}.v1.0~"), ".*", ".a._.a.v0"),
        (
            format!("{RESOURCE}.v{}~", "9".repeat(80)),
            ".*",
            ".a._.a.v0",
        ),
        (bucket, ".*", ".a._.a.v0"),
    ]
}

#[test]
fn empty_selections_are_covered_by_any_operand() {
    let operand = "gts.zzz.*";
    for (prefix, wildcard, completion) in boundary_shapes() {
        // Pads a name so that the shortest completion has `total` characters.
        let fill = |total: usize| {
            let name = "a".repeat(total - prefix.len() - completion.len());
            (
                format!("{prefix}{name}{wildcard}"),
                format!("{prefix}{name}{completion}"),
            )
        };
        let (selecting, witness) = fill(GTS_ID_MAX_LENGTH);
        assert_eq!(witness.len(), GTS_ID_MAX_LENGTH);
        assert!(
            covers(&selecting, &witness),
            "{selecting} selects {witness}"
        );
        assert!(!covers(operand, &selecting), "{selecting} is not empty");

        let (empty, too_long) = fill(GTS_ID_MAX_LENGTH + 1);
        assert!(GtsPattern::parse(&too_long).is_err());
        assert!(covers(operand, &empty), "{empty} selects nothing");
        assert!(covers(&empty, &empty), "{empty} covers itself");
        assert!(!covers(&empty, &selecting), "{empty} omits {witness}");
    }
}

fn holder(operand: &str) -> Value {
    json!({
        "$id": "gts://gts.x.test.pr.holder.v1~",
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "single": {"type": "string", "x-gts-pattern-ref": operand},
            "many": {"type": "array", "items": {"type": "string", "x-gts-pattern-ref": operand}},
            "negated": {"not": {"x-gts-pattern-ref": operand}}
        }
    })
}

fn is_valid(schema: &Value, instance: &Value) -> bool {
    crate::json_schema::gts_validator_for(schema, None)
        .expect("compiles")
        .is_valid(instance)
}

#[test]
fn keyword_checks_strings_arrays_and_not() {
    let schema = holder(&format!("{RESOURCE}.*"));
    assert!(is_valid(
        &schema,
        &json!({"single": format!("{RESOURCE}.v1~")})
    ));
    assert!(!is_valid(
        &schema,
        &json!({"single": "gts.x.core.resources.*"})
    ));
    assert!(!is_valid(
        &schema,
        &json!({"single": "gts.x.core.resources.resource.v1*"})
    ));
    assert!(is_valid(
        &schema,
        &json!({"many": [format!("{RESOURCE}.v2~"), format!("{RESOURCE}.v1.*")]})
    ));
    assert!(!is_valid(
        &schema,
        &json!({"many": [format!("{RESOURCE}.v2~"), "gts.*"]})
    ));
    // Malformed and uncovered values are ordinary failures `not` inverts.
    assert!(is_valid(&schema, &json!({"negated": "gts.vendor*"})));
    assert!(is_valid(&schema, &json!({"negated": "gts.*"})));
    assert!(!is_valid(
        &schema,
        &json!({"negated": format!("{RESOURCE}.v1~")})
    ));
    // Strings only.
    let untyped = json!({"x-gts-pattern-ref": "gts.*"});
    for value in [json!(1), json!(null), json!([]), json!({}), json!(true)] {
        assert!(is_valid(&untyped, &value));
    }
}

#[test]
fn invalid_operands_are_schema_errors_even_under_not() {
    for operand in [
        json!("gts.x*"),
        json!("/$id/x"),
        json!("#/$id"),
        json!(1),
        json!("gts://gts.*"),
    ] {
        let schema = json!({"not": {"x-gts-pattern-ref": operand}});
        assert!(
            crate::json_schema::gts_validator_for(&schema, None).is_err(),
            "{operand}"
        );
        assert!(check_operands(&schema).is_err(), "{operand}");
    }
    // `/$id` needs a selected type.
    let anonymous = json!({"type": "string", "x-gts-pattern-ref": "/$id"});
    assert!(crate::json_schema::gts_validator_for(&anonymous, None).is_err());
    assert!(check_operands(&anonymous).is_err());
    // Operands are checked where no validator would reach them.
    let unreached = json!({"$id": "gts://gts.x.a.b.c.v1~", "definitions": {"d": {"x-gts-pattern-ref": "gts.*.x"}}});
    assert!(check_operands(&unreached).is_err());
    assert!(check_operands(&holder("/$id")).is_ok());
}

#[test]
fn self_operand_names_the_selected_leaf() {
    let base = holder("/$id");
    let leaf = "gts.x.test.pr.holder.v1~x.test._.leaf.v1~";
    let validator =
        crate::json_schema::gts_validator_for_type(&base, Some(leaf), &[], None).expect("compiles");
    assert!(validator.is_valid(&json!({"single": format!("{leaf}x.test._.thing.v1")})));
    assert!(validator.is_valid(&json!({"single": format!("{leaf}*")})));
    assert!(!validator.is_valid(&json!({"single": "gts.x.test.pr.holder.v1~"})));
    assert!(!validator.is_valid(&json!({"single": "gts.x.test.pr.holder.v1~x.test._.other.v1~"})));
}

fn property(operand: &Value) -> Value {
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {"p": operand}
    })
}

fn verdicts(old: &Value, new: &Value) -> (CompatibilityVerdict, CompatibilityVerdict) {
    let old = property(old);
    let new = property(new);
    (
        check_backward_compatibility(&old, &new).0,
        check_forward_compatibility(&old, &new).0,
    )
}

#[test]
fn evolution_compares_operands_by_coverage() {
    use CompatibilityVerdict::{Compatible, Incompatible, Unknown};
    let narrow = json!({"type": "string", "x-gts-pattern-ref": format!("{RESOURCE}.v1~")});
    let wide = json!({"type": "string", "x-gts-pattern-ref": format!("{RESOURCE}.*")});
    let equal = json!({"type": "string", "x-gts-pattern-ref": format!("{RESOURCE}.v1.*")});
    let plain = json!({"type": "string"});
    assert_eq!(verdicts(&narrow, &wide), (Compatible, Incompatible));
    assert_eq!(verdicts(&wide, &narrow), (Incompatible, Compatible));
    assert_eq!(verdicts(&narrow, &equal), (Compatible, Compatible));
    assert_eq!(verdicts(&plain, &narrow), (Incompatible, Compatible));

    let not_narrow =
        json!({"type": "string", "not": {"x-gts-pattern-ref": format!("{RESOURCE}.v1~")}});
    let not_wide = json!({"type": "string", "not": {"x-gts-pattern-ref": format!("{RESOURCE}.*")}});
    assert_eq!(verdicts(&not_narrow, &not_wide), (Incompatible, Compatible));
    assert_eq!(verdicts(&not_wide, &not_narrow), (Compatible, Incompatible));

    let any_of = |operand: &str| json!({"anyOf": [{"type": "string", "x-gts-pattern-ref": operand}, {"type": "integer"}]});
    assert_eq!(
        verdicts(
            &any_of(&format!("{RESOURCE}.v1~")),
            &any_of(&format!("{RESOURCE}.*"))
        ),
        (Unknown, Unknown)
    );

    // `allOf` intersects; the narrower operand decides.
    let all_of = json!({"allOf": [wide, narrow]});
    assert_eq!(verdicts(&all_of, &narrow), (Compatible, Compatible));
    // `const` inside the scope is a narrowing.
    let constant = json!({"type": "string", "const": format!("{RESOURCE}.v1~x.y._.z.v1~")});
    assert_eq!(verdicts(&constant, &narrow).0, Compatible);
    let outside = json!({"type": "string", "const": "gts.*"});
    assert_ne!(verdicts(&outside, &narrow).0, Compatible);
}

#[test]
fn evolution_keeps_an_ordinary_not_beside_a_negated_operand() {
    use CompatibilityVerdict::{Compatible, Incompatible, Unknown};
    let negated = json!({"type": "string", "not": {"x-gts-pattern-ref": "gts.x.*"}});
    // "hello" is no pattern, so `negated` accepts it; `ordinary` does not.
    let ordinary = json!({"type": "string", "not": {"const": "hello"}});
    assert_eq!(verdicts(&negated, &ordinary), (Unknown, Unknown));
    assert_eq!(verdicts(&ordinary, &negated), (Unknown, Unknown));
    // Dropping the negated operand alone stays decidable.
    let plain = json!({"type": "string"});
    assert_eq!(verdicts(&negated, &plain), (Compatible, Incompatible));

    let base = json!({
        "$id": "gts://gts.x.test.pr.base.v1~",
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {"p": ordinary}
    });
    let derived = json!({
        "$id": "gts://gts.x.test.pr.base.v1~x.test._.leaf.v1~",
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {"p": negated}
    });
    assert!(
        !crate::schema_derivation::validate_derivation(&base, &derived, "base", "derived")
            .is_empty()
    );
}

#[test]
fn evolution_reads_a_negated_operand_by_what_its_subschema_matches() {
    use CompatibilityVerdict::{Compatible, Incompatible, Unknown};
    let integer = json!({"type": "integer"});
    // The keyword holds for every non-string: unrestricted, `not` rejects them all.
    let strings_only =
        json!({"type": "integer", "not": {"type": "string", "x-gts-pattern-ref": "gts.*"}});
    let everything = json!({"type": "integer", "not": {"x-gts-pattern-ref": "gts.*"}});
    assert_eq!(verdicts(&integer, &strings_only), (Compatible, Compatible));
    assert_eq!(verdicts(&integer, &everything), (Incompatible, Compatible));
    assert_eq!(
        verdicts(
            &json!({"type": "integer", "not": {"type": "string", "x-gts-pattern-ref": "gts.x.*"}}),
            &strings_only
        ),
        (Compatible, Compatible)
    );
    // Both accept nothing; the empty source is not established here.
    assert_eq!(
        verdicts(
            &json!({"type": "integer", "not": {"x-gts-pattern-ref": "gts.x.*"}}),
            &everything
        ),
        (Unknown, Compatible)
    );

    // Other keywords in the negated subschema may hide every witness.
    let string = json!({"type": "string"});
    for extra in [
        json!({"pattern": "^zzz"}),
        json!({"const": "gts.x.a.b.c.v1~"}),
    ] {
        let mut negated = json!({"type": "string", "x-gts-pattern-ref": "gts.*"});
        negated
            .as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        let new = json!({"type": "string", "not": negated});
        assert_eq!(verdicts(&string, &new), (Unknown, Compatible), "{extra}");
    }
    // An unrestricted negation still breaks a string source.
    let string_negated = json!({"type": "string", "not": {"x-gts-pattern-ref": "gts.*"}});
    assert_eq!(
        verdicts(&string, &string_negated),
        (Incompatible, Compatible)
    );
}

#[test]
fn derivation_conjoins_inherited_operands() {
    let base = json!({
        "$id": "gts://gts.x.test.pr.base.v1~",
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {"p": {"type": "string", "x-gts-pattern-ref": format!("{RESOURCE}.v1~")}}
    });
    let derived = |overlay: &str| {
        json!({
            "$id": "gts://gts.x.test.pr.base.v1~x.test._.leaf.v1~",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                base.clone(),
                {"type": "object", "properties": {"p": {"type": "string", "x-gts-pattern-ref": overlay}}}
            ]
        })
    };
    let admit = |derived: &Value| {
        crate::schema_derivation::validate_derivation(&base, derived, "base", "derived")
    };
    // A wider overlay does not loosen the inherited scope; a narrower one narrows.
    assert!(admit(&derived(&format!("{RESOURCE}.*"))).is_empty());
    assert!(admit(&derived(&format!("{RESOURCE}.v1~x.*"))).is_empty());
    // Standalone, the wider declaration is not included in the base.
    let standalone = json!({
        "$id": "gts://gts.x.test.pr.base.v1~x.test._.leaf.v1~",
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {"p": {"type": "string", "x-gts-pattern-ref": format!("{RESOURCE}.*")}}
    });
    assert!(!admit(&standalone).is_empty());
    // `/$id` resolves per effective schema: the leaf's scope is inside the base's.
    // Without the negated property, whose direction `not` reverses.
    let mut self_base = holder("/$id");
    self_base["properties"]
        .as_object_mut()
        .expect("properties")
        .remove("negated");
    let mut self_leaf = self_base.clone();
    self_leaf["$id"] = json!("gts://gts.x.test.pr.holder.v1~x.test._.leaf.v1~");
    assert!(
        crate::schema_derivation::validate_derivation(&self_base, &self_leaf, "b", "d").is_empty()
    );
    assert!(
        !crate::schema_derivation::validate_derivation(&self_leaf, &self_base, "b", "d").is_empty()
    );
}
