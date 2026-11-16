// gts:ignore - its unit tests embed annotated sources in string literals.
//! `#[struct_to_gts_schema]` declarations of one source file, read with `syn`.
//! Macro bodies (`macro_rules!`, `quote!`) are opaque and contribute nothing.

use proc_macro2::{Delimiter, TokenTree};
use syn::spanned::Spanned;
use syn::visit::Visit;

/// A module-level declaration the compiled exporter is expected to report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decl {
    pub name: String,
    /// Inline `mod` names from the file's module down to the declaration.
    pub mods: Vec<String>,
    pub line: usize,
    pub overrides: bool,
    /// Own generic bounds, a where-clause, lifetimes or const parameters: the
    /// macro emits no exporter.
    pub bounded: bool,
}

/// A declaration inside a function body or other block; never exported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDecl {
    pub name: String,
    pub line: usize,
}

#[derive(Debug, Default)]
pub struct Inventory {
    pub decls: Vec<Decl>,
    pub locals: Vec<LocalDecl>,
}

impl Inventory {
    pub fn has_overrides(&self) -> bool {
        self.decls.iter().any(|d| d.overrides)
    }
}

/// # Errors
///
/// The file is not valid Rust syntax.
pub fn scan(content: &str) -> syn::Result<Inventory> {
    let file = syn::parse_file(content)?;
    let mut visitor = Visitor::default();
    visitor.visit_file(&file);
    Ok(visitor.inventory)
}

#[derive(Default)]
struct Visitor {
    inventory: Inventory,
    mods: Vec<String>,
    block_depth: usize,
}

impl<'ast> Visit<'ast> for Visitor {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.mods.push(item.ident.to_string());
        syn::visit::visit_item_mod(self, item);
        self.mods.pop();
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.block_depth += 1;
        syn::visit::visit_block(self, block);
        self.block_depth -= 1;
    }

    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        if let Some(attr) = item.attrs.iter().find(|a| is_schema_attr(a)) {
            let name = item.ident.to_string();
            let line = attr.span().start().line;
            if self.block_depth > 0 {
                self.inventory.locals.push(LocalDecl { name, line });
            } else {
                let generics = &item.generics;
                self.inventory.decls.push(Decl {
                    name,
                    mods: self.mods.clone(),
                    line,
                    overrides: has_overrides_arg(attr),
                    bounded: generics.where_clause.is_some()
                        || generics.params.iter().any(|param| match param {
                            syn::GenericParam::Type(p) => !p.bounds.is_empty(),
                            _ => true,
                        }),
                });
            }
        }
        syn::visit::visit_item_struct(self, item);
    }
}

fn is_schema_attr(attr: &syn::Attribute) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|s| s.ident == "struct_to_gts_schema")
}

/// A top-level `overrides(...)` argument of the attribute.
fn has_overrides_arg(attr: &syn::Attribute) -> bool {
    let syn::Meta::List(list) = &attr.meta else {
        return false;
    };
    let args: Vec<TokenTree> = list.tokens.clone().into_iter().collect();
    args.windows(2).enumerate().any(|(i, pair)| {
        let at_arg_start =
            i == 0 || matches!(&args[i - 1], TokenTree::Punct(p) if p.as_char() == ',');
        at_arg_start
            && matches!(&pair[0], TokenTree::Ident(id) if id == "overrides")
            && matches!(&pair[1], TokenTree::Group(g) if g.delimiter() == Delimiter::Parenthesis)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"
#[struct_to_gts_schema(dir_path = "schemas", base = true, type_id = "a~")]
pub struct BaseV1<P> { pub payload: P }

/// overrides(BaseV1 = Helper) in a doc comment
#[struct_to_gts_schema(dir_path = "schemas", base = BaseV1, type_id = "a~b~",
    description = "overrides(BaseV1 = Helper)")]
pub struct PlainV1 { pub x: String }

mod orders {
    #[gts_macros::struct_to_gts_schema(
        dir_path = "schemas",
        base = BaseV1,
        type_id = gts_id!("a~c~"),
        overrides(BaseV1 = { #[schemars(length(max = 3))] x: String })
    )]
    pub struct PlacedV1 { pub y: String }

    #[struct_to_gts_schema(dir_path = "schemas", base = true, type_id = "d~")]
    pub struct BoundedV1<P: Clone> { pub payload: P }

    #[struct_to_gts_schema(dir_path = "schemas", base = true, type_id = "e~")]
    pub struct DefaultedV1<P = ()> { pub payload: P }
}

macro_rules! template {
    () => {
        #[struct_to_gts_schema(dir_path = "schemas", base = true, type_id = "t~",
            overrides(X = Y))]
        pub struct TemplateV1;
    };
}

fn local() {
    #[struct_to_gts_schema(dir_path = "schemas", base = true, type_id = "l~")]
    struct LocalV1;
}
"#;

    #[test]
    fn scans_module_level_declarations() {
        let inventory = scan(SOURCE).unwrap();
        let summary: Vec<_> = inventory
            .decls
            .iter()
            .map(|d| (d.name.as_str(), d.mods.join("::"), d.overrides, d.bounded))
            .collect();
        assert_eq!(
            summary,
            [
                ("BaseV1", String::new(), false, false),
                ("PlainV1", String::new(), false, false),
                ("PlacedV1", "orders".to_owned(), true, false),
                ("BoundedV1", "orders".to_owned(), false, true),
                ("DefaultedV1", "orders".to_owned(), false, false),
            ]
        );
        let line_of = |needle: &str| SOURCE.lines().position(|l| l.contains(needle)).unwrap() + 1;
        assert_eq!(inventory.decls[2].line, line_of("#[gts_macros::"));
        assert!(inventory.has_overrides());
        assert_eq!(
            inventory.locals,
            [LocalDecl {
                name: "LocalV1".to_owned(),
                line: line_of("type_id = \"l~\"")
            }]
        );
    }

    #[test]
    fn invalid_syntax_is_an_error() {
        assert!(scan("pub struct {").is_err());
    }
}
