#![feature(rustc_private)]

dylint_linting::dylint_library!();

extern crate rustc_ast;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_lint;
extern crate rustc_session;
extern crate rustc_span;

use rustc_ast::LitKind;
use rustc_hir::ExprKind;
use rustc_lint::{LateContext, LateLintPass, LintContext};
use rustc_session::declare_lint;

const LINT_MESSAGE: &str = "hard-coded GTS ID prefix in string literal - use GTS_ID_PREFIX constant or the gts_id! macro instead";

declare_lint! {
    /// Lint that flags hard-coded GTS identifier prefixes in string literals.
    ///
    /// String literals starting with a configured prefix (default: `"gts."`)
    /// should use the `GTS_ID_PREFIX` constant from the `gts-id` crate, so that
    /// the prefix remains configurable via the `GTS_ID_PREFIX` environment variable.
    ///
    /// For constructing GTS IDs at compile time, use the `gts_id!` macro from
    /// the `gts-macros` crate, which automatically applies the configured prefix.
    ///
    /// The set of flagged prefixes can be customized at lint-load time via the
    /// `GTS_DYLINT_PREFIXES` environment variable (comma-separated, e.g.
    /// `GTS_DYLINT_PREFIXES="gts.,acme."`). Defaults to `gts.` and also includes
    /// the active `GTS_ID_PREFIX` value when that environment variable is set.
    ///
    /// Macros whose expansions may legitimately produce prefixed literals
    /// (`gts_id`, `struct_to_gts_schema`, `gts_instance`, `gts_instance_raw`)
    /// are always allowed. Additional wrapper macro names can be registered
    /// via the `GTS_DYLINT_ALLOWED_MACROS` environment variable (comma-separated,
    /// e.g. `GTS_DYLINT_ALLOWED_MACROS="my_gts_id,my_schema"`).
    ///
    /// The default level is `deny`. To override, use standard Rust lint
    /// attributes: `#[warn(gts_id_hardcoded_prefix)]`, `#[allow(...)]`, or
    /// `--cap-lints` on the command line.
    ///
    /// To suppress this lint in specific cases (e.g. constant definitions or
    /// test data), use `#[allow(gts_id_hardcoded_prefix)]`.
    pub GTS_ID_HARDCODED_PREFIX,
    Deny,
    LINT_MESSAGE
}

rustc_session::declare_lint_pass!(GtsIdHardcodedPrefix => [GTS_ID_HARDCODED_PREFIX]);

fn push_unique(values: &mut Vec<String>, value: &str) {
    let trimmed = value.trim();
    if !trimmed.is_empty() && !values.iter().any(|v| v == trimmed) {
        values.push(trimmed.to_owned());
    }
}

fn comma_separated_values(value: &str) -> impl Iterator<Item = &str> {
    value.split(',').map(str::trim).filter(|s| !s.is_empty())
}

fn configured_prefixes_from(
    lint_prefixes: Option<&str>,
    active_gts_id_prefix: Option<&str>,
) -> Vec<String> {
    let mut prefixes = Vec::new();
    match lint_prefixes {
        Some(value) if !value.trim().is_empty() => {
            for prefix in comma_separated_values(value) {
                push_unique(&mut prefixes, prefix);
            }
        }
        _ => prefixes.push("gts.".to_owned()),
    }
    if let Some(prefix) = active_gts_id_prefix {
        push_unique(&mut prefixes, prefix);
    }
    prefixes
}

/// Returns the list of prefixes to flag, read from `GTS_DYLINT_PREFIXES` and
/// `GTS_ID_PREFIX`. Defaults to `["gts."]`.
fn configured_prefixes() -> Vec<String> {
    let lint_prefixes = std::env::var("GTS_DYLINT_PREFIXES").ok();
    let active_prefix = std::env::var("GTS_ID_PREFIX").ok();
    configured_prefixes_from(lint_prefixes.as_deref(), active_prefix.as_deref())
}

/// Returns the list of macro names whose expansions are allowed to produce
/// prefixed string literals without triggering the lint. Built-in GTS macros
/// are always included; additional names (e.g. project-specific wrapper
/// macros) can be added via the `GTS_DYLINT_ALLOWED_MACROS` environment
/// variable (comma-separated, e.g. `GTS_DYLINT_ALLOWED_MACROS="my_gts_id,my_schema"`).
fn configured_allowed_macros_from(extra_names: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = vec![
        "gts_id".into(),
        "struct_to_gts_schema".into(),
        "gts_instance".into(),
        "gts_instance_raw".into(),
    ];
    if let Some(value) = extra_names {
        for name in comma_separated_values(value) {
            push_unique(&mut names, name);
        }
    }
    names
}

fn configured_allowed_macros() -> Vec<String> {
    let extra_names = std::env::var("GTS_DYLINT_ALLOWED_MACROS").ok();
    configured_allowed_macros_from(extra_names.as_deref())
}

static PREFIXES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
static ALLOWED_MACROS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

fn get_prefixes() -> &'static [String] {
    PREFIXES.get_or_init(configured_prefixes)
}

fn get_allowed_macros() -> &'static [String] {
    ALLOWED_MACROS.get_or_init(configured_allowed_macros)
}

#[unsafe(no_mangle)]
pub fn register_lints(_sess: &rustc_session::Session, lint_store: &mut rustc_lint::LintStore) {
    lint_store.register_lints(&[GTS_ID_HARDCODED_PREFIX]);
    lint_store.register_late_pass(|_| Box::new(GtsIdHardcodedPrefix));
}

impl<'tcx> LateLintPass<'tcx> for GtsIdHardcodedPrefix {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx rustc_hir::Expr<'tcx>) {
        if let ExprKind::Lit(lit) = &expr.kind
            && let LitKind::Str(symbol, _) = lit.node
        {
            let s = symbol.as_str();
            if !get_prefixes().iter().any(|p| s.starts_with(p.as_str())) {
                return;
            }

            // Skip string literals produced by the `gts_id!` macro, which
            // legitimately applies the configured prefix at compile time.
            // Other macros (e.g. `concat!`) that produce prefixed strings are
            // still flagged: they bypass the configurable prefix mechanism.
            if expr.span.from_expansion() {
                let allowed = get_allowed_macros();
                let is_from_allowed_macro = expr.span.macro_backtrace().any(|expn| {
                    if let rustc_span::ExpnKind::Macro(_, name) = expn.kind {
                        allowed.iter().any(|a| name.as_str() == a.as_str())
                    } else {
                        false
                    }
                });
                if is_from_allowed_macro {
                    return;
                }
                // For non-gts_id macros (e.g. concat!), emit the lint at the
                // call-site span so the lint level is resolved there, not at
                // the macro definition site (which may suppress it).
                let span = expr.span.source_callsite();
                cx.opt_span_lint(
                    GTS_ID_HARDCODED_PREFIX,
                    Some(span),
                    rustc_errors::DiagDecorator(|diag| {
                        diag.primary_message(LINT_MESSAGE);
                    }),
                );
                return;
            }

            let span = expr.span;
            cx.opt_span_lint(
                GTS_ID_HARDCODED_PREFIX,
                Some(span),
                rustc_errors::DiagDecorator(|diag| {
                    diag.primary_message(LINT_MESSAGE);
                }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{configured_allowed_macros_from, configured_prefixes_from};

    #[test]
    fn examples() {
        dylint_testing::ui_test_examples(env!("CARGO_PKG_NAME"));
    }

    #[test]
    fn prefixes_default_to_gts_and_include_active_gts_id_prefix() {
        assert_eq!(configured_prefixes_from(None, None), ["gts."]);
        assert_eq!(
            configured_prefixes_from(None, Some("acme.")),
            ["gts.", "acme."]
        );
    }

    #[test]
    fn prefixes_honor_explicit_list_and_deduplicate_active_prefix() {
        assert_eq!(
            configured_prefixes_from(Some("gts., acme., acme., "), Some("acme.")),
            ["gts.", "acme."]
        );
        assert_eq!(
            configured_prefixes_from(Some("corp."), Some("acme.")),
            ["corp.", "acme."]
        );
    }

    #[test]
    fn allowed_macros_include_builtins_and_configured_wrappers() {
        assert_eq!(
            configured_allowed_macros_from(Some("my_gts_id, my_schema, gts_id")),
            [
                "gts_id",
                "struct_to_gts_schema",
                "gts_instance",
                "gts_instance_raw",
                "my_gts_id",
                "my_schema",
            ]
        );
    }
}
