//! A hand-written generic `GtsSchema` has no macro-emitted constant to pick its
//! slot, so a bare name cannot be resolved: write `HandV1<()>`.

use gts_macros::struct_to_gts_schema;

pub struct HandV1<P>(std::marker::PhantomData<P>);

impl<P> gts::GtsSchema for HandV1<P> {
    const TYPE_ID: &'static str = "gts.x.app.entities.hand.v1~";
    const GENERIC_FIELD: Option<&'static str> = Some("payload");

    fn gts_schema_with_refs() -> serde_json::Value {
        serde_json::json!({})
    }
}

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = HandV1,
    type_id = "gts.x.app.entities.hand.v1~x.app.entities.child.v1~",
    description = "Bare hand-written generic parent",
    properties = "x"
)]
pub struct ChildV1 {
    pub x: String,
}

fn main() {}
