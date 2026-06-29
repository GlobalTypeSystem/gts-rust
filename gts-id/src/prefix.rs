//! Configuration of the GTS identifier prefix.
//!
//! The prefix is resolved once, at compile time, from the [`GTS_ID_PREFIX_ENV`]
//! environment variable (falling back to [`DEFAULT_GTS_ID_PREFIX`]). It is
//! validated by [`validate_gts_id_prefix`] so an invalid override fails the
//! build instead of silently producing malformed identifiers.
//!
//! Because the value is baked in at compile time, the crate's `build.rs` emits
//! `rerun-if-env-changed` so Cargo rebuilds when the variable changes.

/// The default identifier prefix for all GTS identifiers.
#[allow(unknown_lints, gts_id_hardcoded_prefix)]
pub const DEFAULT_GTS_ID_PREFIX: &str = "gts.";

/// Environment variable used to override the GTS identifier prefix at compile time.
///
/// `option_env!` requires a string *literal*, so the name is repeated in
/// [`GTS_ID_PREFIX`]'s initializer below; `build.rs` likewise hardcodes it for
/// its `rerun-if-env-changed` hint. Keep all three occurrences in sync.
pub const GTS_ID_PREFIX_ENV: &str = "GTS_ID_PREFIX";

/// The configured prefix for all GTS identifiers.
///
/// Defaults to [`DEFAULT_GTS_ID_PREFIX`] and can be overridden at compile time
/// via the [`GTS_ID_PREFIX_ENV`] environment variable. The override must be a
/// single lowercase token (`[a-z][a-z0-9_]*`) terminated by a single `.`
/// (e.g. `acme.`); multi-segment prefixes such as `my.org.` are rejected at
/// compile time by [`validate_gts_id_prefix`].
pub const GTS_ID_PREFIX: &str = validate_gts_id_prefix(match option_env!("GTS_ID_PREFIX") {
    Some(prefix) => prefix,
    None => DEFAULT_GTS_ID_PREFIX,
});

/// Validates a configured GTS identifier prefix at compile time.
///
/// A valid prefix is a single lowercase token (`[a-z][a-z0-9_]*`) followed by
/// a single trailing `.`. Multi-segment prefixes (`my.org.`), uppercase, and
/// other punctuation are rejected.
#[allow(clippy::manual_is_ascii_check)]
const fn validate_gts_id_prefix(prefix: &str) -> &str {
    let bytes = prefix.as_bytes();
    assert!(!bytes.is_empty(), "GTS_ID_PREFIX must not be empty");
    assert!(
        bytes[bytes.len() - 1] == b'.',
        "GTS_ID_PREFIX must end with '.'"
    );
    assert!(
        bytes.len() != 1,
        "GTS_ID_PREFIX must contain a token before the final dot"
    );

    let first = bytes[0];
    assert!(
        first >= b'a' && first <= b'z',
        "GTS_ID_PREFIX must start with a lowercase ASCII letter"
    );

    let mut i = 1;
    while i < bytes.len() - 1 {
        let b = bytes[i];
        assert!(
            (b >= b'a' && b <= b'z') || (b >= b'0' && b <= b'9') || b == b'_',
            "GTS_ID_PREFIX must be a lowercase ASCII token followed by '.'"
        );
        i += 1;
    }

    prefix
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::panic::catch_unwind;

    #[test]
    fn test_valid_prefixes() {
        assert_eq!(validate_gts_id_prefix("gts."), "gts.");
        assert_eq!(validate_gts_id_prefix("a."), "a.");
        assert_eq!(validate_gts_id_prefix("acme."), "acme.");
        assert_eq!(validate_gts_id_prefix("a1."), "a1.");
        assert_eq!(validate_gts_id_prefix("a_b."), "a_b.");
        assert_eq!(validate_gts_id_prefix("abc123_."), "abc123_.");
    }

    #[test]
    fn test_default_prefix_is_in_effect() {
        // When no custom prefix is configured via GTS_ID_PREFIX, the resolved
        // prefix must equal the default. When a custom prefix *is* configured,
        // the resolved prefix differs from the default — that's expected and
        // tested by the compile-time validation above.
        if option_env!("GTS_ID_PREFIX").is_none() {
            assert_eq!(GTS_ID_PREFIX, DEFAULT_GTS_ID_PREFIX);
            assert_eq!(GTS_ID_PREFIX, "gts.");
        }
    }

    #[test]
    fn test_invalid_prefixes_rejected() {
        let invalid = [
            "",           // empty
            "acme",       // no trailing dot
            "Acme.",      // uppercase
            "acme-prod.", // hyphen
            "my.org.",    // multi-segment (dot in middle)
            ".",          // bare dot, no token
            "1bad.",      // starts with digit
            "_.",         // starts with underscore
        ];
        for prefix in invalid {
            let result = catch_unwind(|| validate_gts_id_prefix(prefix));
            assert!(
                result.is_err(),
                "prefix {prefix:?} should be rejected but was accepted"
            );
        }
    }
}
