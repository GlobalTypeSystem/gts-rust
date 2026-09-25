use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
};
use gts::GtsOps;
use gts::GtsRefValidation;
use gts::ops::AddEntityRejection;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

use crate::logging::LoggingMiddleware;

#[derive(Clone)]
pub struct AppState {
    pub ops: Arc<Mutex<GtsOps>>,
}

pub struct GtsHttpServer {
    ops: GtsOps,
    host: String,
    port: u16,
    verbose: u8,
}

impl GtsHttpServer {
    #[must_use]
    pub fn new(ops: GtsOps, host: String, port: u16, verbose: u8) -> Self {
        Self {
            ops,
            host,
            port,
            verbose,
        }
    }

    /// Run the HTTP server
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The server fails to bind to the specified address
    /// - The server encounters an error while serving requests
    pub async fn run(self) -> anyhow::Result<()> {
        let verbose = self.verbose;
        let state = AppState {
            ops: Arc::new(Mutex::new(self.ops)),
        };

        let app = Self::create_router(state, verbose);

        let addr = format!("{}:{}", self.host, self.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;

        tracing::info!("Server listening on {}", addr);
        axum::serve(listener, app).await?;

        Ok(())
    }

    pub fn create_router(state: AppState, verbose: u8) -> Router {
        let mut router = Router::new()
            .route("/entities", get(get_entities).post(add_entity))
            .route("/entities/{gts_id}", get(get_entity))
            .route("/entities/bulk", post(add_entities))
            .route("/type-schemas", post(add_schemas))
            .route("/validate-id", get(validate_id))
            .route("/extract-id", post(extract_id))
            .route("/parse-id", get(parse_id))
            .route("/match-id-pattern", get(match_id_pattern))
            .route("/uuid", get(id_to_uuid))
            .route("/validate-instance", post(validate_instance))
            .route("/validate-type-schema", post(validate_schema))
            .route("/validate-entity", post(validate_entity))
            .route("/validate-json", post(validate_json))
            .route("/validate-json/{gts_type}", post(validate_json_as_type))
            .route("/resolve-relationships", get(schema_graph))
            .route("/compatibility", get(compatibility))
            .route("/cast", post(cast))
            .route("/query", get(query))
            .route("/attr", get(attr))
            .with_state(state);

        // Add custom logging middleware if verbose >= 1
        if verbose >= 1 {
            let logging = LoggingMiddleware::new(verbose);
            router = router.layer(middleware::from_fn(move |req, next| {
                let logging = logging.clone();
                async move { logging.handle(req, next).await }
            }));
        }

        router
    }

    #[must_use]
    pub fn openapi_spec(&self) -> Value {
        json!({
            "openapi": "3.0.0",
            "info": {
                "title": "GTS Server",
                "version": "0.1.0"
            },
            "servers": [{
                "url": format!("http://{}:{}", self.host, self.port)
            }],
            "paths": {
                "/entities": {
                    "get": { "summary": "Get all entities in the registry" },
                    "post": { "summary": "Register a single entity" }
                },
                "/validate-id": {
                    "get": { "summary": "Validate GTS identifier" }
                }
            }
        })
    }
}

// Query parameters
#[derive(Deserialize)]
struct GtsIdQuery {
    gts_id: String,
}

#[derive(Deserialize)]
struct MatchIdQuery {
    candidate: String,
    pattern: String,
}

#[derive(Deserialize)]
struct CompatibilityQuery {
    old_type_id: String,
    new_type_id: String,
}

#[derive(Deserialize)]
struct QueryParams {
    expr: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Deserialize)]
struct AttrQuery {
    gts_with_path: String,
}

#[derive(Deserialize)]
struct LimitQuery {
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Deserialize)]
struct AddEntityQuery {
    #[serde(default)]
    validate: bool,
    #[serde(flatten)]
    refs: GtsRefValidationQuery,
}

/// The `gts-ref-validation` request parameter (spec v0.14 §9.6).
#[derive(Deserialize, Default)]
struct GtsRefValidationQuery {
    #[serde(rename = "gts-ref-validation")]
    mode: Option<String>,
}

impl GtsRefValidationQuery {
    /// The requested mode, or why the spelling was rejected.
    fn resolve(&self) -> Result<GtsRefValidation, String> {
        self.mode
            .as_deref()
            .map_or_else(|| Ok(GtsRefValidation::default()), GtsRefValidation::parse)
    }
}

fn unprocessable(error: &str) -> axum::response::Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({"ok": false, "error": error})),
    )
        .into_response()
}

fn default_limit() -> usize {
    100
}

#[derive(Deserialize, serde::Serialize)]
struct CastRequest {
    instance_id: String,
    to_type_id: String,
}

#[derive(Deserialize, serde::Serialize)]
struct ValidateInstanceRequest {
    instance_id: String,
}

#[derive(Deserialize, serde::Serialize)]
struct ValidateTypeSchemaRequest {
    type_id: String,
}

#[derive(Deserialize, serde::Serialize)]
struct ValidateEntityRequest {
    #[serde(alias = "gts_id")]
    entity_id: String,
}

// Helper function to lock mutex or return error response
fn lock_ops(
    mutex: &Arc<Mutex<GtsOps>>,
) -> Result<std::sync::MutexGuard<'_, GtsOps>, impl IntoResponse> {
    mutex.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Server state corrupted"})),
        )
    })
}

// Async Handlers
async fn get_entities(
    State(state): State<AppState>,
    Query(params): Query<LimitQuery>,
) -> impl IntoResponse {
    let ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.get_entities(params.limit);
    Json(result).into_response()
}

async fn get_entity(
    State(state): State<AppState>,
    Path(gts_id): Path<String>,
) -> impl IntoResponse {
    let mut ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.get_entity(&gts_id);
    Json(result).into_response()
}

async fn add_entity(
    State(state): State<AppState>,
    Query(params): Query<AddEntityQuery>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let refs = match params.refs.resolve() {
        Ok(refs) => refs,
        Err(error) => return unprocessable(&error),
    };
    let mut ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.add_entity_with(&body, params.validate, refs);
    let status = match (result.ok, result.rejection) {
        (true, _) => StatusCode::OK,
        (false, Some(AddEntityRejection::Conflict)) => StatusCode::CONFLICT,
        (false, None) => StatusCode::UNPROCESSABLE_ENTITY,
    };
    (status, Json(result)).into_response()
}

async fn add_entities(
    State(state): State<AppState>,
    Json(body): Json<Vec<Value>>,
) -> impl IntoResponse {
    let mut ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.add_entities(&body);
    Json(result).into_response()
}

/// Per-entry outcomes are in the body, so a partly rejected batch is still a
/// 200; only a body that is not an array of schemas is refused outright.
async fn add_schemas(
    State(state): State<AppState>,
    Json(body): Json<Vec<Value>>,
) -> impl IntoResponse {
    let mut ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.add_schemas(&body);
    Json(result).into_response()
}

async fn validate_id(
    State(state): State<AppState>,
    Query(params): Query<GtsIdQuery>,
) -> impl IntoResponse {
    let _ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = GtsOps::validate_id(&params.gts_id);
    Json(result).into_response()
}

async fn extract_id(State(state): State<AppState>, Json(body): Json<Value>) -> impl IntoResponse {
    let ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.extract_id(&body);
    Json(result).into_response()
}

async fn parse_id(
    State(state): State<AppState>,
    Query(params): Query<GtsIdQuery>,
) -> impl IntoResponse {
    let _ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = GtsOps::parse_id(&params.gts_id);
    Json(result).into_response()
}

async fn match_id_pattern(
    State(state): State<AppState>,
    Query(params): Query<MatchIdQuery>,
) -> impl IntoResponse {
    let _ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = GtsOps::match_id_pattern(&params.candidate, &params.pattern);
    Json(result).into_response()
}

async fn id_to_uuid(
    State(state): State<AppState>,
    Query(params): Query<GtsIdQuery>,
) -> impl IntoResponse {
    let _ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = GtsOps::uuid(&params.gts_id);
    Json(result).into_response()
}

async fn validate_instance(
    State(state): State<AppState>,
    Query(params): Query<GtsRefValidationQuery>,
    Json(body): Json<ValidateInstanceRequest>,
) -> impl IntoResponse {
    let refs = match params.resolve() {
        Ok(refs) => refs,
        Err(error) => return unprocessable(&error),
    };
    let mut ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.validate_instance_with(&body.instance_id, refs);
    Json(result).into_response()
}

async fn validate_schema(
    State(state): State<AppState>,
    Query(params): Query<GtsRefValidationQuery>,
    Json(body): Json<ValidateTypeSchemaRequest>,
) -> impl IntoResponse {
    let refs = match params.resolve() {
        Ok(refs) => refs,
        Err(error) => return unprocessable(&error),
    };
    let mut ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.validate_schema_with(&body.type_id, refs);
    Json(result).into_response()
}

async fn validate_entity(
    State(state): State<AppState>,
    Query(params): Query<GtsRefValidationQuery>,
    Json(body): Json<ValidateEntityRequest>,
) -> impl IntoResponse {
    let refs = match params.resolve() {
        Ok(refs) => refs,
        Err(error) => return unprocessable(&error),
    };
    let mut ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.validate_entity_with(&body.entity_id, refs);
    Json(result).into_response()
}

async fn validate_json(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Map<String, Value>>,
) -> impl IntoResponse {
    let mut ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.validate_json(&Value::Object(body));
    Json(result).into_response()
}

async fn validate_json_as_type(
    State(state): State<AppState>,
    Path(gts_type): Path<String>,
    Json(body): Json<serde_json::Map<String, Value>>,
) -> impl IntoResponse {
    let mut ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.validate_json_as_type(&gts_type, &Value::Object(body));
    Json(result).into_response()
}

async fn schema_graph(
    State(state): State<AppState>,
    Query(params): Query<GtsIdQuery>,
) -> impl IntoResponse {
    let mut ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.schema_graph(&params.gts_id);
    Json(result).into_response()
}

async fn compatibility(
    State(state): State<AppState>,
    Query(params): Query<CompatibilityQuery>,
) -> impl IntoResponse {
    let mut ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.compatibility(&params.old_type_id, &params.new_type_id);
    Json(result).into_response()
}

async fn cast(State(state): State<AppState>, Json(body): Json<CastRequest>) -> impl IntoResponse {
    let mut ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.cast(&body.instance_id, &body.to_type_id);
    Json(result).into_response()
}

async fn query(
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> impl IntoResponse {
    let ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.query(&params.expr, params.limit);
    Json(result).into_response()
}

async fn attr(State(state): State<AppState>, Query(params): Query<AttrQuery>) -> impl IntoResponse {
    let mut ops = match lock_ops(&state.ops) {
        Ok(guard) => guard,
        Err(response) => return response.into_response(),
    };
    let result = ops.attr(&params.gts_with_path);
    Json(result).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_limit() {
        assert_eq!(default_limit(), 100);
    }

    #[test]
    fn test_lock_ops_success() {
        let ops = GtsOps::new(None, None, 0);
        let state = Arc::new(Mutex::new(ops));

        let result = lock_ops(&state);
        assert!(result.is_ok());
    }

    #[test]
    fn test_app_state_creation() {
        let ops = GtsOps::new(None, None, 0);
        let _state = AppState {
            ops: Arc::new(Mutex::new(ops)),
        };

        // AppState is Clone, verified by compilation
    }

    #[test]
    fn test_gts_http_server_creation() {
        let ops = GtsOps::new(None, None, 0);
        let server = GtsHttpServer::new(ops, "127.0.0.1".to_owned(), 8080, 0);

        // Server is created successfully - just verify struct construction
        assert_eq!(server.host, "127.0.0.1");
        assert_eq!(server.port, 8080);
        assert_eq!(server.verbose, 0);
    }
}
