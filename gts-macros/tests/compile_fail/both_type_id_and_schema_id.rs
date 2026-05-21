//! Test: specifying both `type_id` and the deprecated `schema_id` alias must error.

use gts_macros::struct_to_gts_schema;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    type_id = "gts.x.test.alias.foo.v1~",
    schema_id = "gts.x.test.alias.foo.v1~",
    description = "Both attribute forms used together",
    properties = "id"
)]
pub struct Foo {
    pub id: String,
}

fn main() {}
