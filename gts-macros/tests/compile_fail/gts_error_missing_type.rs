use gts_macros::gts_error;

#[gts_error(
    status = 404,
    title = "Not Found",
)]
pub struct MissingTypeError {
    pub gts_id: String,
}

fn main() {}
