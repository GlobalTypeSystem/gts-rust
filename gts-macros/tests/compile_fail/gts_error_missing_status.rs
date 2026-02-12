use gts_macros::gts_error;

#[gts_error(
    r#type = "cf.system.logical.not_found.v1",
    title = "Not Found",
)]
pub struct MissingStatusError {
    pub gts_id: String,
}

fn main() {}
