//! Test: GTS type/id field present on struct but not listed in properties should fail

use gts_macros::struct_to_gts_schema;
use gts::gts::GtsSchemaId;

#[struct_to_gts_schema(
    dir_path = "schemas",
    base = true,
    schema_id = "gts.x.core.errors.rate_limit.v1~",
    description = "Rate limit error with gts_type not in properties",
    properties = "retry_after"
)]
#[derive(Debug)]
pub struct RateLimitErrorV1 {
    pub gts_type: GtsSchemaId,
    pub retry_after: u64,
}

fn main() {}
