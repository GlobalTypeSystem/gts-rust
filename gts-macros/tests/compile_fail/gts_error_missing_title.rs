use gts_macros::gts_error;

#[gts_error(
    r#type = "cf.system.logical.not_found.v1",
    status = 404,
)]
pub struct MissingTitleError {
    pub gts_id: String,
}

fn main() {}
