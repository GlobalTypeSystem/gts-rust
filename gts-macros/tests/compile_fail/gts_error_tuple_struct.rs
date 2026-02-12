use gts_macros::gts_error;

#[gts_error(
    r#type = "cf.system.logical.not_found.v1",
    status = 404,
    title = "Not Found",
)]
pub struct TupleStructError(String, u32);

fn main() {}
