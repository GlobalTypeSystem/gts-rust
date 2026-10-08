use super::*;

/// The supported constructs of README §11.0.1 and expressions at the bounds.
fn in_profile() -> Vec<String> {
    let mut expressions: Vec<String> = [
        "",
        "abc",
        "é😀",
        "^[A-Za-z0-9]+$",
        "[^abc]",
        "(foo|bar)+",
        "(?:ab)+",
        "[a-z]{1,3}",
        "a{3}",
        "a{2,}",
        "(a(b)?c)*",
        "a.*?b",
        "a*?b+?c??d{1,2}?e{2,}?f{2}?",
        r"\(x\)\.\*",
        r"\\C",
        r"\n\r\t\f\v",
        r"\x41",
        r"\d\D\w\W\s\S",
        "^a.c$",
        "(a*)*b",
        "(a+)+$",
        "^(a|aa)+$",
        r"^(?:(a|aa)+$|a+!$)",
        "a{1000}",
        "(?:a{10}){100}",
        "a{1000,}",
        "a{0}",
        "a{0,0}",
        "a{10}",
        "(?:a{0,1000}){1}",
        "|",
        "a|",
        "()",
        "(?:)*",
        r"\^\$\\\.\*\+\?\(\)\[\]\{\}\|\/",
        r"[\^\$\\\.\*\+\?\(\)\[\]\{\}\|\/\-]",
        "[-a]",
        "[a-]",
        "[^-a]",
        "[a-c-]",
        "[a^$.*+?(){}|]",
        r"[\d\s\w\D\S\W]",
        r"[\x41-\x5A\n]",
        r"[!-\-]",
        r"[\--0]",
        "[&-~]",
        "(?:a{20}|b{30}){30}",
        "(?:a{1000}|b{1000})",
        // A zero count encloses a counted child as a factor of one.
        "(?:a{1000}){0}",
        "(?:a{1000}){0,}",
        "[😀-😂é]",
        "a-b/c,d&e~f#g h",
        "[&~]",
        "^P[^\\n\\r\u{2028}\u{2029}]+",
        "\u{0}",
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    expressions.push(format!("{}{}", "a{1000}".repeat(4), "a".repeat(72)));
    expressions.push("a".repeat(4096));
    expressions.push("é".repeat(4096));
    expressions.push("😀".repeat(4096));
    let deepest = MAX_GROUP_NESTING;
    expressions.push(format!("{}a{}", "(".repeat(deepest), ")".repeat(deepest)));
    expressions.push(format!("{}a{}", "(?:".repeat(deepest), ")".repeat(deepest)));
    // Each level nests a repetition, a group, an alternation and a
    // concatenation, the most the engine's parser counts per group.
    expressions.push(format!(
        "{}X{}",
        "(a|a".repeat(deepest),
        ")*".repeat(deepest)
    ));
    // The same, with root alternation and a class of translated shorthands.
    expressions.push(format!(
        r"z|x{}[\s\S\w\W\d\D]*?{}",
        "(a|a".repeat(deepest),
        ")*".repeat(deepest)
    ));
    expressions
}

#[test]
fn supports_the_profile() {
    for expression in in_profile() {
        assert_eq!(check(&expression), Ok(()), "{expression}");
    }
}

#[test]
fn rejects_malformed_expressions() {
    for expression in [
        "[",
        "[unclosed",
        "(unclosed",
        "a)",
        "a{3,2}",
        "\\",
        "*abc",
        "a|*",
        "(*a)",
        "[z-a]",
        "a{",
        "a{1",
        "a{1,",
        "a{1,2",
        r"\x4",
        r"\xZZ",
    ] {
        assert!(check(expression).is_err(), "{expression}");
    }
}

#[test]
fn rejects_syntax_outside_the_common_subset() {
    for expression in [
        // ECMA-262 `u` only.
        "a(?=b)",
        "a(?!b)",
        "(?<=a)b",
        "(?<!a)b",
        r"^(a+)\1$",
        r"^(?<w>a)\k<w>$",
        // The regex escape backslash-u, not a literal `A`.
        concat!("\\", "u0041"),
        r"\cA",
        r"\0",
        "[]a]",
        "[^]",
        "[]",
        r"[\b]",
        // RE2 only.
        r"\Qx.y\E",
        "(?U)a+",
        "(?i)abc",
        r"abc\z",
        r"\Aabc",
        r"\x{41}",
        "[[:alpha:]]",
        r"\C",
        r"\pL",
        "a{,5}",
        "a{1, 2}",
        // RE2 reads a leading zero as a literal `{`.
        "a{01}",
        "a{1,02}",
        // Neither, or read differently.
        "(?>a+)b",
        "a++b",
        "a**",
        "a*??",
        "a{2}{3}",
        "a{2}*",
        "^*",
        "$?",
        "a}",
        "a]",
        "{1}",
        r"\-",
        r"\a",
        r"\e",
        r"\%",
        "[a[b]]",
        "[a&&b]",
        "[a--b]",
        "[a~~b]",
        // Rust reads a doubled `-`, `&` or `~` as a set operation even where
        // it follows or ends a range.
        "[!--0]",
        "[&--&]",
        r"[\x21--0]",
        r"[\---0]",
        "[!-&&a]",
        "[!-~~a]",
        "[^!--0]",
        "[--a]",
        "[a-c-e]",
        r"[\d-z]",
        r"[a-\d]",
        r"[\x41-\d]",
        // Excluded by the draft.
        "(?i:a)b",
        "(?<w>a)",
        "(?P<w>a)",
        r"^\p{L}+$",
        r"\ba\b",
        r"\Ba",
    ] {
        assert!(check(expression).is_err(), "{expression}");
    }
}

#[test]
fn rejects_expressions_beyond_the_bounds() {
    for expression in [
        "a{1001}".to_owned(),
        "a{99999999999999999999999}".to_owned(),
        "(?:a{1000}){2}".to_owned(),
        "(?:a{1000,}){2}".to_owned(),
        "(?:(?:a{10}){10}){11}".to_owned(),
        format!("{}{}", "a{1000}".repeat(4), "a".repeat(73)),
        "a".repeat(4097),
        "é".repeat(4097),
        "(?:ab){1000}".to_owned(),
        format!(
            "{}a{}",
            "(".repeat(MAX_GROUP_NESTING + 1),
            ")".repeat(MAX_GROUP_NESTING + 1)
        ),
        // The bound applies to the expansion, whatever the source length.
        "(?:(?:abcd){30}){30}".to_owned(),
        // Small expansions whose nested products exceed the bound.
        "(?:a{10}){101}".to_owned(),
        "(?:(?:a{2}){2}){251}".to_owned(),
        "(?:a{1,}){1001}".to_owned(),
    ] {
        assert!(check(&expression).is_err(), "{expression}");
    }
}

#[test]
fn counts_the_expanded_length_as_specified() {
    // `X{n}` counts its spelling plus `max(n, 1)` copies of `X`.
    let pad = |head: String, expanded: usize| format!("{head}{}", "a".repeat(4096 - expanded));
    let exact = pad("(?:ab){100}".to_owned(), 5 + 100 * 6);
    assert_eq!(check(&exact), Ok(()));
    assert!(check(&format!("{exact}a")).is_err());
    // `X{n,m}` counts `max(m, 1)` copies.
    let bounded = pad("a{2,999}".to_owned(), 7 + 999);
    assert_eq!(check(&bounded), Ok(()));
    assert!(check(&format!("{bounded}a")).is_err());
    // `X{n,}` counts `n + 1` copies.
    let open = pad("a{999,}".to_owned(), 6 + 1000);
    assert_eq!(check(&open), Ok(()));
    assert!(check(&format!("{open}a")).is_err());
    // A zero count still counts one copy, and a lazy suffix its spelling.
    let zero = pad("a{0}?".to_owned(), 4 + 1);
    assert_eq!(check(&zero), Ok(()));
    assert!(check(&format!("{zero}a")).is_err());
}

#[test]
fn every_supported_expression_compiles_for_the_linear_engine() {
    for expression in in_profile() {
        crate::schema_regex::compile_for_engine(&expression)
            .unwrap_or_else(|e| panic!("{expression}: {e}"));
    }
    assert!(crate::schema_regex::compile_for_engine("(").is_err());
}

#[test]
fn counts_escapes_and_classes_by_their_spelling() {
    // `\x41` spells four code points and `[ab]` four, not one each.
    let escaped = format!("\\x41{{1000}}{}", "a".repeat(4096 - 6 - 4 * 1000));
    assert_eq!(check(&escaped), Ok(()));
    assert!(check(&format!("{escaped}a")).is_err());
    let class = format!("[ab]{{1000}}{}", "a".repeat(4096 - 6 - 4 * 1000));
    assert_eq!(check(&class), Ok(()));
    assert!(check(&format!("{class}a")).is_err());
}

#[test]
fn concatenated_repetitions_add_up_without_multiplying() {
    // Siblings: each path has product 1000, and the lengths add up.
    let siblings = "(?:a{10}){100}(?:b{10}){100}";
    assert_eq!(check(siblings), Ok(()));
    let nested = format!("(?:{siblings}){{2}}");
    assert!(
        check(&nested).is_err(),
        "the product doubles along each path"
    );
}

#[test]
fn nesting_of_groups_and_alternations_compiles_for_the_linear_engine() {
    // Groups, alternations and repetitions each nest in the engine's parser.
    let deep = format!("{}a{}", "(?:b|(".repeat(16), ")*)*".repeat(16));
    assert_eq!(check(&deep), Ok(()));
    let translated = jsonschema_regex::to_rust_regex(&deep).expect("translates");
    regex::Regex::new(&translated).expect("compiles");
}

#[test]
fn rejects_an_overlong_source_before_parsing_it() {
    for overlong in [
        "a".repeat(4097),
        format!("[{}]", "a".repeat(5000)),
        format!("({}", "a".repeat(5000)),
    ] {
        assert!(check(&overlong).is_err());
    }
}
