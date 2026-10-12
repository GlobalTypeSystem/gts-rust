use super::*;

/// Prepend the configured prefix to a suffix, yielding a full GTS id string
/// for use in tests. This keeps tests prefix-aware without hardcoding "gts.".
fn gts_id(suffix: &str) -> String {
    format!("{}{suffix}", crate::GTS_ID_PREFIX)
}

#[test]
fn test_pattern_matching_checks_the_whole_candidate_scope() {
    let instance = gts_id("x.a.b.c.v1~acme.prod._.uploads.v2");
    let filter = GtsIdPattern::try_new(&instance).expect("test");
    let candidate = GtsIdPattern::try_new(&format!("{instance}.*")).expect("test");
    assert!(!candidate.matches_pattern(&filter));

    let broad = GtsIdPattern::try_new(&gts_id("x.a.b.c.v1~")).expect("test");
    assert!(candidate.matches_pattern(&broad));
    assert!(!broad.matches_pattern(&candidate));
}

#[test]
fn test_empty_candidate_pattern_matches_any_filter() {
    let empty = format!(
        "{}{}.*",
        crate::GTS_ID_PREFIX,
        "a".repeat(crate::GTS_ID_MAX_LENGTH - crate::GTS_ID_PREFIX.len() - 2)
    );
    let candidate = GtsIdPattern::try_new(&empty).expect("test");
    let filter = GtsIdPattern::try_new(&gts_id("zzz.*")).expect("test");
    assert!(candidate.matches_pattern(&filter));
    assert!(!filter.matches_pattern(&candidate));
}

#[test]
fn test_gts_wildcard_simple() {
    let pattern = GtsIdPattern::try_new(&gts_id("x.core.events.*")).expect("test");
    let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
    assert!(id.matches_pattern(&pattern));
}

#[test]
fn test_gts_wildcard_no_match() {
    let pattern = GtsIdPattern::try_new(&gts_id("x.core.events.*")).expect("test");
    let id = GtsId::try_new(&gts_id("y.core.events.event.v1~")).expect("test");
    assert!(!id.matches_pattern(&pattern));
}

#[test]
fn test_trailing_wildcard_ignores_type_marker() {
    // A trailing `*` matches the candidate position whether the candidate
    // segment there is a type (`~`) or an instance. A wildcard segment never
    // carries its own type marker, so it imposes no `is_type` constraint.
    let pattern = GtsIdPattern::try_new(&gts_id("x.core.events.topic.v1~*")).expect("test");

    let type_candidate = GtsId::try_new(&gts_id(
        "x.core.events.topic.v1~vendor.app.orders.thing.v1~",
    ))
    .expect("test");
    let instance_candidate = GtsId::try_new(&gts_id(
        "x.core.events.topic.v1~vendor.app.orders.thing.v1.0",
    ))
    .expect("test");

    assert!(type_candidate.matches_pattern(&pattern));
    assert!(instance_candidate.matches_pattern(&pattern));
}

#[test]
fn test_trailing_chain_wildcard_matches_empty_suffix() {
    let pattern = GtsIdPattern::try_new(&gts_id("x.core.events.topic.v1~*")).expect("test");
    let exact = GtsId::try_new(&gts_id("x.core.events.topic.v1~")).expect("test");
    let specific_minor = GtsId::try_new(&gts_id("x.core.events.topic.v1.1~")).expect("test");

    assert!(exact.matches_pattern(&pattern));
    assert!(specific_minor.matches_pattern(&pattern));
}

#[test]
fn test_prefixed_chain_wildcard_requires_a_suffix() {
    let pattern = GtsIdPattern::try_new(&gts_id("x.core.events.topic.v1~abc.*")).expect("test");
    let base = GtsId::try_new(&gts_id("x.core.events.topic.v1~")).expect("test");

    assert!(!base.matches_pattern(&pattern));
}

#[test]
fn test_zero_major_minor_wildcard_is_scoped_to_v0() {
    let pattern = GtsIdPattern::try_new(&gts_id("x.core.events.topic.v0.*")).expect("test");
    let v0 = GtsId::try_new(&gts_id("x.core.events.topic.v0.2~")).expect("test");
    let v1 = GtsId::try_new(&gts_id("x.core.events.topic.v1.2~")).expect("test");

    assert_eq!(pattern.segments()[0].ver_major_opt(), Some(0));
    assert!(v0.matches_pattern(&pattern));
    assert!(!v1.matches_pattern(&pattern));
}

#[test]
fn test_pattern_version_range_is_u32() {
    for suffix in [
        "x.core.events.event.v4294967295~",
        "x.core.events.event.v1.4294967295~",
        "x.core.events.event.v4294967295.*",
    ] {
        assert!(GtsIdPattern::try_new(&gts_id(suffix)).is_ok(), "{suffix}");
    }
    for suffix in [
        "x.core.events.event.v4294967296~",
        "x.core.events.event.v1.4294967296~",
        "x.core.events.event.v4294967296.*",
        "x.core.events.event.v1~acme.prod._.uploads.v4294967296",
    ] {
        assert!(GtsIdPattern::try_new(&gts_id(suffix)).is_err(), "{suffix}");
    }
}

#[test]
fn test_minor_wildcard_requires_an_explicit_minor() {
    for (prefix, ending) in [
        ("", "~"),
        ("x.core.events.event.v1~", ""),
        ("x.core.events.event.v1~", "~"),
    ] {
        let pattern = GtsIdPattern::try_new(&gts_id(&format!("{prefix}acme.prod._.uploads.v2.*")))
            .expect("test");
        let omitted = gts_id(&format!("{prefix}acme.prod._.uploads.v2{ending}"));
        let explicit = gts_id(&format!("{prefix}acme.prod._.uploads.v2.3{ending}"));
        assert!(
            !GtsId::try_new(&omitted)
                .expect("test")
                .matches_pattern(&pattern),
            "{omitted}"
        );
        assert!(
            GtsId::try_new(&explicit)
                .expect("test")
                .matches_pattern(&pattern),
            "{explicit}"
        );
        assert!(!pattern.covers(&GtsIdPattern::try_new(&omitted).expect("test")));
        assert!(pattern.covers(&pattern));
        if prefix.is_empty() {
            assert!(
                GtsIdPattern::try_new(&omitted)
                    .expect("test")
                    .covers(&pattern)
            );
        }
    }
}

#[test]
fn test_gts_wildcard_type_suffix() {
    // Wildcard after ~ should match type IDs
    let pattern = GtsIdPattern::try_new(&gts_id("x.core.events.*")).expect("test");
    let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
    assert!(id.matches_pattern(&pattern));
}

#[test]
fn test_version_flexibility_in_matching() {
    // Pattern without minor version should match any minor version
    let pattern = GtsIdPattern::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
    let id_no_minor = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
    let id_with_minor = GtsId::try_new(&gts_id("x.core.events.event.v1.0~")).expect("test");

    assert!(id_no_minor.matches_pattern(&pattern));
    assert!(id_with_minor.matches_pattern(&pattern));
}

#[test]
fn test_gts_wildcard_exact_match() {
    let pattern = GtsIdPattern::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
    let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
    assert!(id.matches_pattern(&pattern));
}

#[test]
fn test_gts_wildcard_version_mismatch() {
    let pattern = GtsIdPattern::try_new(&gts_id("x.core.events.event.v2~")).expect("test");
    let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
    assert!(!id.matches_pattern(&pattern));
}

#[test]
fn test_pattern_longer_than_candidate_does_not_match() {
    let pattern = GtsIdPattern::try_new(&gts_id(
        "x.core.events.topic.v1~vendor.app.orders.order.v1~",
    ))
    .expect("test");
    let id = GtsId::try_new(&gts_id("x.core.events.topic.v1~")).expect("test");
    assert!(!id.matches_pattern(&pattern));
}

#[test]
fn test_uuid_tail_mismatch_does_not_match() {
    let pattern = GtsIdPattern::try_new(&gts_id(
        "x.core.events.topic.v1~7a1d2f34-5678-49ab-9012-abcdef123456",
    ))
    .expect("test");
    let id = GtsId::try_new(&gts_id(
        "x.core.events.topic.v1~7a1d2f34-5678-49ab-9012-abcdef123457",
    ))
    .expect("test");
    assert!(!id.matches_pattern(&pattern));
}

#[test]
fn test_gts_wildcard_with_minor_version() {
    let pattern = GtsIdPattern::try_new(&gts_id("x.core.events.event.v1.0~")).expect("test");
    let id = GtsId::try_new(&gts_id("x.core.events.event.v1.0~")).expect("test");
    assert!(id.matches_pattern(&pattern));
}

#[test]
fn test_gts_wildcard_invalid_pattern() {
    let result = GtsIdPattern::try_new("invalid");
    assert!(result.is_err());
}

#[test]
fn test_gts_wildcard_multiple_wildcards_error() {
    let result = GtsIdPattern::try_new(&gts_id("*.*.*.*"));
    assert!(result.is_err());
}

#[test]
fn test_gts_wildcard_instance_match() {
    let pattern = GtsIdPattern::try_new(&gts_id("x.core.events.*")).expect("test");
    let id = GtsId::try_new(&gts_id("x.core.events.event.v1~a.b.c.d.v1.0")).expect("test");
    assert!(id.matches_pattern(&pattern));
}

#[test]
fn test_gts_pattern_rejects_surrounding_whitespace() {
    for suffix in [
        "x.core.events.*",
        "x.core.events.event.v1~",
        "x.core.events.event.v1~x.core._.item.v1",
    ] {
        let canonical = gts_id(suffix);
        for (leading, trailing) in [
            (" ", ""),
            ("", " "),
            ("\t", "\r\n"),
            ("\u{a0}", "\u{a0}"),
            ("", "\u{2028}"),
        ] {
            let input = format!("{leading}{canonical}{trailing}");
            assert!(GtsIdPattern::try_new(&input).is_err(), "{input:?}");
            assert!(input.parse::<GtsIdPattern>().is_err(), "{input:?}");
            assert!(!GtsIdPattern::is_valid(&input), "{input:?}");
        }
    }
}

#[test]
fn test_gts_wildcard_only_at_end() {
    // Wildcard in middle should fail
    let result1 = GtsIdPattern::try_new(&gts_id("*.core.events.event.v1~"));
    assert!(result1.is_err());

    // Wildcard at end should work
    let pattern2 = GtsIdPattern::try_new(&gts_id("x.core.events.*")).expect("test");
    let id2 = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
    assert!(id2.matches_pattern(&pattern2));
}

#[test]
fn test_gts_wildcard_no_wildcard_different_vendor() {
    let pattern = GtsIdPattern::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
    let id = GtsId::try_new(&gts_id("y.core.events.event.v1~")).expect("test");
    assert!(!id.matches_pattern(&pattern));
}

#[test]
fn test_gts_wildcard_display_trait() {
    let pattern = GtsIdPattern::try_new(&gts_id("x.core.events.*")).expect("test");
    assert_eq!(format!("{pattern}"), gts_id("x.core.events.*"));
}

#[test]
fn test_gts_wildcard_from_str_trait() {
    let pattern: GtsIdPattern = gts_id("x.core.events.*").parse().expect("test");
    assert_eq!(pattern.pattern(), gts_id("x.core.events.*"));
}

#[test]
fn test_gts_wildcard_as_ref_trait() {
    let pattern = GtsIdPattern::try_new(&gts_id("x.core.events.*")).expect("test");
    let s: &str = pattern.as_ref();
    assert_eq!(s, gts_id("x.core.events.*"));
}

#[test]
fn test_gts_wildcard_type_suffix_match() {
    // Wildcard after type suffix
    let pattern = GtsIdPattern::try_new(&gts_id("x.pkg.ns.type.v1~*")).expect("test");
    let id1 = GtsId::try_new(&gts_id("x.pkg.ns.type.v1~a.b.c.child.v1~")).expect("test");
    let id2 = GtsId::try_new(&gts_id("x.pkg.ns.type.v2~a.b.c.child.v1~")).expect("test");
    assert!(id1.matches_pattern(&pattern));
    assert!(!id2.matches_pattern(&pattern));
}

#[test]
fn test_gts_wildcard_at_various_positions() {
    // Wildcard at vendor position
    let result = GtsIdPattern::try_new(&gts_id("*"));
    assert!(result.is_ok());

    // Wildcard at package position
    let result = GtsIdPattern::try_new(&gts_id("x.*"));
    assert!(result.is_ok());

    // Wildcard at namespace position
    let result = GtsIdPattern::try_new(&gts_id("x.pkg.*"));
    assert!(result.is_ok());

    // Wildcard at type position
    let result = GtsIdPattern::try_new(&gts_id("x.pkg.ns.*"));
    assert!(result.is_ok());

    // Wildcard at version position
    let result = GtsIdPattern::try_new(&gts_id("x.pkg.ns.type.*"));
    assert!(result.is_ok());
}

// ---- covers ----

#[test]
fn test_covers_broad_covers_narrow() {
    let broad = GtsIdPattern::try_new(&gts_id("x.core.srr.resource.v1~*")).expect("test");
    let narrow = GtsIdPattern::try_new(&gts_id("x.core.srr.resource.v1~acme.*")).expect("test");
    // Coverage is directional: the broad pattern covers the narrow one,
    // never the reverse.
    assert!(broad.covers(&narrow));
    assert!(!narrow.covers(&broad));
}

#[test]
fn test_covers_disjoint_types() {
    let a = GtsIdPattern::try_new(&gts_id("x.core.srr.resource.v1~*")).expect("test");
    let b = GtsIdPattern::try_new(&gts_id("x.core.other.resource.v1~*")).expect("test");
    assert!(!a.covers(&b));
    assert!(!b.covers(&a));
}

#[test]
fn test_covers_identical_patterns() {
    let a = GtsIdPattern::try_new(&gts_id("x.core.srr.resource.v1~*")).expect("test");
    let b = GtsIdPattern::try_new(&gts_id("x.core.srr.resource.v1~*")).expect("test");
    // A pattern covers an identical one (both directions).
    assert!(a.covers(&b));
    assert!(b.covers(&a));
}

#[test]
fn test_covers_wildcard_covers_exact() {
    let exact = GtsIdPattern::try_new(&gts_id("x.core.srr.resource.v1~acme.crm._.contact.v1~"))
        .expect("test");
    let broad = GtsIdPattern::try_new(&gts_id("x.core.srr.resource.v1~*")).expect("test");
    assert!(broad.covers(&exact));
    assert!(!exact.covers(&broad));
}

#[test]
fn test_covers_three_levels() {
    let l1 = GtsIdPattern::try_new(&gts_id("x.core.srr.resource.v1~*")).expect("test");
    let l2 = GtsIdPattern::try_new(&gts_id("x.core.srr.resource.v1~acme.*")).expect("test");
    let l3 = GtsIdPattern::try_new(&gts_id("x.core.srr.resource.v1~acme.crm.*")).expect("test");
    // Broader patterns cover narrower ones, transitively.
    assert!(l1.covers(&l2));
    assert!(l1.covers(&l3));
    assert!(l2.covers(&l3));
    assert!(!l2.covers(&l1));
    assert!(!l3.covers(&l1));
    assert!(!l3.covers(&l2));
}

// ---- glued version wildcard `v*` ----

#[test]
fn test_version_wildcard_valid_and_is_wildcard() {
    let pattern = GtsIdPattern::try_new(&gts_id("x.llm.chat.message.v*")).expect("test");
    assert_eq!(pattern.segments().len(), 1);
    assert!(pattern.segments()[0].is_wildcard());
    assert!(GtsIdPattern::is_valid(&gts_id("x.llm.chat.message.v*")));
}

#[test]
fn test_version_wildcard_matches_any_version_and_chain() {
    let pattern = GtsIdPattern::try_new(&gts_id("x.llm.chat.message.v*")).expect("test");
    for id in [
        gts_id("x.llm.chat.message.v1.0~"),
        gts_id("x.llm.chat.message.v1.1~"),
        gts_id("x.llm.chat.message.v2~"),
        gts_id("x.llm.chat.message.v1.0~acme.app.ns.derived.v1~"),
    ] {
        let candidate = GtsId::try_new(&id).expect("test");
        assert!(candidate.matches_pattern(&pattern), "should match: {id}");
    }
    // Different type is not matched.
    let other = GtsId::try_new(&gts_id("x.llm.chat.other.v1~")).expect("test");
    assert!(!other.matches_pattern(&pattern));
}

#[test]
fn test_version_wildcard_equivalent_to_version_position_star() {
    // `message.v*` and `message.*` match the same set: the `v` marker adds no
    // constraint because every version token starts with `v`.
    let v_star = GtsIdPattern::try_new(&gts_id("x.llm.chat.message.v*")).expect("test");
    let dot_star = GtsIdPattern::try_new(&gts_id("x.llm.chat.message.*")).expect("test");
    for id in [
        gts_id("x.llm.chat.message.v1~"),
        gts_id("x.llm.chat.message.v9.9~"),
        gts_id("x.llm.chat.message.v1.0~acme.app.ns.derived.v1~"),
    ] {
        let candidate = GtsId::try_new(&id).expect("test");
        assert_eq!(
            candidate.matches_pattern(&v_star),
            candidate.matches_pattern(&dot_star),
            "match divergence for {id}"
        );
    }
}

#[test]
fn test_version_wildcard_rejections() {
    // `*` after `v*` — two wildcards.
    assert!(!GtsIdPattern::is_valid(&gts_id("x.llm.chat.message.v*~*")));
    // A stray `~` after the wildcard — `*` is not the final character.
    assert!(!GtsIdPattern::is_valid(&gts_id("x.llm.chat.message.v1.*~")));
    // Partial (non-version) token wildcard.
    assert!(!GtsIdPattern::is_valid(&gts_id("x.llm.chat.msg*")));
}

// ---- covers: minor-version flexibility (segment-based) ----

#[test]
fn test_covers_any_minor_covers_specific_minor() {
    // A pattern pinned to a major (no minor) is broader than one pinned to a
    // specific minor — segment-based coverage captures this; string prefixes
    // would not (`…v1~` is not a string prefix of `…v1.0~`).
    let any_minor = GtsIdPattern::try_new(&gts_id("x.core.events.event.v1~*")).expect("test");
    let specific = GtsIdPattern::try_new(&gts_id("x.core.events.event.v1.0~*")).expect("test");
    assert!(any_minor.covers(&specific));
    assert!(!specific.covers(&any_minor));
}

#[test]
fn test_covers_bare_type_minor_flexibility() {
    let any_minor = GtsIdPattern::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
    let specific = GtsIdPattern::try_new(&gts_id("x.core.events.event.v1.0~")).expect("test");
    assert!(any_minor.covers(&specific));
    assert!(!specific.covers(&any_minor));
}

#[test]
fn test_covers_major_version_mismatch() {
    let v1 = GtsIdPattern::try_new(&gts_id("x.core.events.event.v1~*")).expect("test");
    let v2 = GtsIdPattern::try_new(&gts_id("x.core.events.event.v2~*")).expect("test");
    assert!(!v1.covers(&v2));
    assert!(!v2.covers(&v1));
}

// ---- From<GtsId> conversion ----

#[test]
fn test_from_gts_id_ref() {
    let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
    let pattern = GtsIdPattern::from(&id);
    // The pattern string is the id verbatim.
    assert_eq!(pattern.pattern(), id.id());
    // A concrete id maps to a concrete pattern — no wildcard segments.
    assert!(pattern.segments().iter().all(|s| !s.is_wildcard()));
    // The id at minimum matches the pattern derived from itself.
    assert!(id.matches_pattern(&pattern));
}

#[test]
fn test_concrete_pattern_covers_derived_chain() {
    // Per GTS spec §3.6 "implicit derived-type coverage": a base type id used
    // as a pattern is treated as the implicit envelope `…~*`, so it matches
    // not only itself but every type/instance derived from it down the chain.
    let pattern = GtsId::try_new(&gts_id("a.b.c.d.v1~"))
        .expect("test")
        .to_pattern();

    // Exact and derived candidates both match.
    let exact = GtsId::try_new(&gts_id("a.b.c.d.v1~")).expect("test");
    let derived = GtsId::try_new(&gts_id("a.b.c.d.v1~w.x.y.z.v1")).expect("test");
    assert!(exact.matches_pattern(&pattern));
    assert!(derived.matches_pattern(&pattern));

    // A different base type is not covered.
    let other_base = GtsId::try_new(&gts_id("a.b.c.other.v1~w.x.y.z.v1")).expect("test");
    assert!(!other_base.matches_pattern(&pattern));
}

#[test]
fn test_from_gts_id_owned() {
    let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
    let expected = id.id().to_owned();
    let pattern: GtsIdPattern = id.into();
    assert_eq!(pattern.pattern(), expected);
    assert!(pattern.segments().iter().all(|s| !s.is_wildcard()));
}

#[test]
fn test_from_gts_id_chained_preserves_segments() {
    let id = GtsId::try_new(&gts_id(
        "x.core.events.topic.v1~vendor.app.orders.thing.v1.0",
    ))
    .expect("test");
    let pattern = GtsIdPattern::from(&id);
    assert_eq!(pattern.segments().len(), id.segments().len());
    assert_eq!(pattern.pattern(), id.id());
    assert!(id.matches_pattern(&pattern));
}

#[test]
fn test_from_ref_and_owned_agree() {
    let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
    let from_ref = GtsIdPattern::from(&id);
    let from_owned = GtsIdPattern::from(id);
    // Borrowing and consuming conversions produce the same pattern.
    assert_eq!(from_ref, from_owned);
}

#[test]
fn test_from_gts_id_matches_to_pattern() {
    let id = GtsId::try_new(&gts_id("x.core.events.event.v1~")).expect("test");
    // The inherent `to_pattern` is just the ergonomic form of `From<&GtsId>`.
    assert_eq!(GtsIdPattern::from(&id), id.to_pattern());
}

// ---- is_valid ----

#[test]
fn test_is_valid() {
    // Exact ids and trailing-`*` patterns are valid.
    assert!(GtsIdPattern::is_valid(&gts_id("x.core.events.event.v1~")));
    assert!(GtsIdPattern::is_valid(&gts_id("x.core.events.*")));
    assert!(GtsIdPattern::is_valid(&gts_id("x.core.events.topic.v1~*")));

    // Malformed strings and misplaced wildcards are not.
    assert!(!GtsIdPattern::is_valid("not-a-gts-id"));
    assert!(!GtsIdPattern::is_valid(&gts_id("x.*.events.event.v1~")));
    assert!(!GtsIdPattern::is_valid(&gts_id(
        "x.core.events.topic.v1.*~"
    )));
}

fn pattern(text: &str) -> GtsIdPattern {
    GtsIdPattern::try_new(text).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn instance_scope_does_not_cover_type_or_wildcard() {
    let instance = format!("{GTS_ID_PREFIX}x.core.events.event.v1~acme.prod._.uploads.v2");
    let scope = pattern(&instance);
    assert!(scope.covers(&pattern(&format!("{instance}.0"))));
    assert!(!scope.covers(&pattern(&format!("{instance}~"))));
    assert!(!scope.covers(&pattern(&format!("{instance}.*"))));
}

#[test]
fn empty_selections_are_covered_at_the_length_limit() {
    let empty = format!(
        "{GTS_ID_PREFIX}{}.*",
        "a".repeat(GTS_ID_MAX_LENGTH - GTS_ID_PREFIX.len() - 2)
    );
    let empty = pattern(&empty);
    let unrelated = pattern(&format!("{GTS_ID_PREFIX}zzz.*"));
    assert!(unrelated.covers(&empty));
    assert!(!empty.covers(&unrelated));
}

#[test]
fn length_limited_minor_wildcard_selects_only_instances() {
    let prefix = format!("{GTS_ID_PREFIX}x.core.resources.resource.v1~");
    let ending = ".b.c.d.v0.*";
    let name = "a".repeat(GTS_ID_MAX_LENGTH - prefix.len() - ending.len());
    let value = format!("{prefix}{name}{ending}");
    let operand = format!("{prefix}{name}.b.c.d.v0");
    assert_eq!(value.len(), GTS_ID_MAX_LENGTH);
    // An explicit minor fits; the type marker or a longer chain would not.
    let value = pattern(&value);
    assert!(pattern(&operand).covers(&value));
    assert!(value.covers(&pattern(&format!("{operand}.0"))));
    assert!(value.covers(&pattern(&format!("{operand}.9"))));
    assert!(!pattern(&format!("{operand}.0")).covers(&value));
    assert!(!value.covers(&pattern(&operand)));
}
