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
    pub requires_compiled_export: bool,
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
            self.inventory.requires_compiled_export |= requires_compiled_export(attr);
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

/// Look only at top-level macro arguments. Commas inside an override, a
/// literal or a macro invocation do not delimit arguments here.
fn requires_compiled_export(attr: &syn::Attribute) -> bool {
    let syn::Meta::List(list) = &attr.meta else {
        return false;
    };
    let args: Vec<TokenTree> = list.tokens.clone().into_iter().collect();
    args.split(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == ','))
        .any(|arg| match arg {
            [TokenTree::Ident(key), TokenTree::Group(body)] => {
                key == "overrides" && body.delimiter() == Delimiter::Parenthesis
            }
            [
                TokenTree::Ident(key),
                TokenTree::Punct(eq),
                TokenTree::Ident(_),
                TokenTree::Punct(open),
                ..,
            ] => key == "base" && eq.as_char() == '=' && open.as_char() == '<',
            _ => false,
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
            .map(|d| (d.name.as_str(), d.mods.join("::"), d.bounded))
            .collect();
        assert_eq!(
            summary,
            [
                ("BaseV1", String::new(), false),
                ("PlainV1", String::new(), false),
                ("PlacedV1", "orders".to_owned(), false),
                ("BoundedV1", "orders".to_owned(), true),
                ("DefaultedV1", "orders".to_owned(), false),
            ]
        );
        let line_of = |needle: &str| SOURCE.lines().position(|l| l.contains(needle)).unwrap() + 1;
        assert_eq!(inventory.decls[2].line, line_of("#[gts_macros::"));
        assert!(inventory.requires_compiled_export);
        assert_eq!(
            inventory.locals,
            [LocalDecl {
                name: "LocalV1".to_owned(),
                line: line_of("type_id = \"l~\"")
            }]
        );
    }

    #[test]
    fn compiled_export_depends_on_macro_arguments() {
        for (args, compiled) in [
            (
                "base = Parent, description = \"base = Parent<()>, overrides(X = Y)\"",
                false,
            ),
            ("base = Parent /* slot */ < ( ) >", true),
            ("base = Parent, overrides(Parent = Rules)", true),
        ] {
            let source = format!("#[struct_to_gts_schema({args})] struct Child;");
            assert_eq!(
                scan(&source).unwrap().requires_compiled_export,
                compiled,
                "{args}"
            );
        }
    }

    #[test]
    fn invalid_syntax_is_an_error() {
        assert!(scan("pub struct {").is_err());
    }
}
