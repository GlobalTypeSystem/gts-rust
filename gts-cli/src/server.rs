use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use gts::GtsOps;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

use crate::logging::LoggingMiddleware;

#[derive(Clone)]
pub struct AppState {
    ops: Arc<Mutex<GtsOps>>,
}

pub struct GtsHttpServer {
    ops: GtsOps,
    host: String,
    port: u16,
    verbose: u8,
}

impl GtsHttpServer {
    pub fn new(ops: GtsOps, host: String, port: u16, verbose: u8) -> Self {
        Self {
            ops,
            host,
            port,
            verbose,
        }
    }

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

    fn create_router(state: AppState, verbose: u8) -> Router {
        let mut router = Router::new()
            .route("/entities", get(get_entities).post(add_entity))
            .route("/entities/{gts_id}", get(get_entity))
            .route("/entities/bulk", post(add_entities))
            .route("/schemas", post(add_schema))
            .route("/validate-id", get(validate_id))
            .route("/extract-id", post(extract_id))
            .route("/parse-id", get(parse_id))
            .route("/match-id-pattern", get(match_id_pattern))
            .route("/uuid", get(id_to_uuid))
            .route("/validate-instance", post(validate_instance))
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
    old_schema_id: String,
    new_schema_id: String,
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
}

fn default_limit() -> usize {
    100
}

#[derive(Deserialize)]
struct SchemaRegister {
    type_id: String,
    #[serde(rename = "schema")]
    schema_content: Value,
}

#[derive(Deserialize)]
struct CastRequest {
    instance_id: String,
    to_schema_id: String,
}

#[derive(Deserialize)]
struct ValidateInstanceRequest {
    instance_id: String,
}

// Async Handlers
async fn get_entities(
    State(state): State<AppState>,
    Query(params): Query<LimitQuery>,
) -> impl IntoResponse {
    let ops = state.ops.lock().unwrap();
    let result = ops.get_entities(params.limit);
    Json(result)
}

async fn get_entity(
    State(state): State<AppState>,
    Path(gts_id): Path<String>,
) -> impl IntoResponse {
    let mut ops = state.ops.lock().unwrap();
    let result = ops.get_entity(&gts_id);
    Json(result)
}

async fn add_entity(
    State(state): State<AppState>,
    Query(params): Query<AddEntityQuery>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let mut ops = state.ops.lock().unwrap();
    let result = ops.add_entity(&body, params.validate);
    if result.ok {
        (StatusCode::OK, Json(result))
    } else {
        (StatusCode::UNPROCESSABLE_ENTITY, Json(result))
    }
}

async fn add_entities(
    State(state): State<AppState>,
    Json(body): Json<Vec<Value>>,
) -> impl IntoResponse {
    let mut ops = state.ops.lock().unwrap();
    let result = ops.add_entities(&body);
    Json(result)
}

async fn add_schema(
    State(state): State<AppState>,
    Json(body): Json<SchemaRegister>,
) -> impl IntoResponse {
    let mut ops = state.ops.lock().unwrap();
    let result = ops.add_schema(body.type_id, &body.schema_content);
    Json(result)
}

async fn validate_id(
    State(state): State<AppState>,
    Query(params): Query<GtsIdQuery>,
) -> impl IntoResponse {
    let ops = state.ops.lock().unwrap();
    let result = ops.validate_id(&params.gts_id);
    Json(result)
}

async fn extract_id(State(state): State<AppState>, Json(body): Json<Value>) -> impl IntoResponse {
    let ops = state.ops.lock().unwrap();
    let result = ops.extract_id(&body);
    Json(result)
}

async fn parse_id(
    State(state): State<AppState>,
    Query(params): Query<GtsIdQuery>,
) -> impl IntoResponse {
    let ops = state.ops.lock().unwrap();
    let result = ops.parse_id(&params.gts_id);
    Json(result)
}

async fn match_id_pattern(
    State(state): State<AppState>,
    Query(params): Query<MatchIdQuery>,
) -> impl IntoResponse {
    let ops = state.ops.lock().unwrap();
    let result = ops.match_id_pattern(&params.candidate, &params.pattern);
    Json(result)
}

async fn id_to_uuid(
    State(state): State<AppState>,
    Query(params): Query<GtsIdQuery>,
) -> impl IntoResponse {
    let ops = state.ops.lock().unwrap();
    let result = ops.uuid(&params.gts_id);
    Json(result)
}

async fn validate_instance(
    State(state): State<AppState>,
    Json(body): Json<ValidateInstanceRequest>,
) -> impl IntoResponse {
    let mut ops = state.ops.lock().unwrap();
    let result = ops.validate_instance(&body.instance_id);
    Json(result)
}

async fn schema_graph(
    State(state): State<AppState>,
    Query(params): Query<GtsIdQuery>,
) -> impl IntoResponse {
    let mut ops = state.ops.lock().unwrap();
    let result = ops.schema_graph(&params.gts_id);
    Json(result)
}

async fn compatibility(
    State(state): State<AppState>,
    Query(params): Query<CompatibilityQuery>,
) -> impl IntoResponse {
    let mut ops = state.ops.lock().unwrap();
    let result = ops.compatibility(&params.old_schema_id, &params.new_schema_id);
    Json(result)
}

async fn cast(State(state): State<AppState>, Json(body): Json<CastRequest>) -> impl IntoResponse {
    let mut ops = state.ops.lock().unwrap();
    let result = ops.cast(&body.instance_id, &body.to_schema_id);
    Json(result)
}

async fn query(
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> impl IntoResponse {
    let ops = state.ops.lock().unwrap();
    let result = ops.query(&params.expr, params.limit);
    Json(result)
}

async fn attr(State(state): State<AppState>, Query(params): Query<AttrQuery>) -> impl IntoResponse {
    let mut ops = state.ops.lock().unwrap();
    let result = ops.attr(&params.gts_with_path);
    Json(result)
}
