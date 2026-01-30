use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use gts::GtsOps;
use gts_cli::server::{AppState, GtsHttpServer};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

fn create_test_ops() -> GtsOps {
    GtsOps::new(None, None, 0)
}

fn create_test_router(ops: GtsOps, verbose: u8) -> Router {
    let state = AppState {
        ops: Arc::new(Mutex::new(ops)),
    };
    GtsHttpServer::create_router(state, verbose)
}

#[tokio::test]
async fn test_openapi_spec_generation() {
    let ops = create_test_ops();
    let server = GtsHttpServer::new(ops, "127.0.0.1".to_owned(), 8000, 0);

    let spec = server.openapi_spec();

    assert!(spec["openapi"].is_string());
    assert_eq!(spec["openapi"], "3.0.0");
    assert!(spec["info"]["title"].is_string());
    assert!(spec["paths"].is_object());
}

#[tokio::test]
async fn test_router_creation_without_logging() {
    let ops = create_test_ops();
    let _app = create_test_router(ops, 0);
    // Just verify it compiles and creates
}

#[tokio::test]
async fn test_router_creation_with_logging() {
    let ops = create_test_ops();
    let _app = create_test_router(ops, 1);
    // Just verify it compiles and creates with middleware
}

#[tokio::test]
async fn test_router_creation_with_verbose_logging() {
    let ops = create_test_ops();
    let _app = create_test_router(ops, 2);
    // Just verify it compiles and creates with verbose middleware
}

#[tokio::test]
async fn test_validate_id_endpoint() {
    let ops = create_test_ops();
    let app = create_test_router(ops, 0);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/validate-id?gts_id=gts.vendor.package.namespace.type.v1.0~")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Schema ID ending with ~ should be valid
    assert_eq!(result["valid"], true);
}

#[tokio::test]
async fn test_validate_id_endpoint_invalid() {
    let ops = create_test_ops();
    let app = create_test_router(ops, 0);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/validate-id?gts_id=invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(result["valid"], false);
}

#[tokio::test]
async fn test_parse_id_endpoint() {
    let ops = create_test_ops();
    let app = create_test_router(ops, 0);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/parse-id?gts_id=gts.vendor:package:schema~")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(result["segments"].is_array());
}

#[tokio::test]
async fn test_match_id_pattern_endpoint() {
    let ops = create_test_ops();
    let app = create_test_router(ops, 0);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/match-id-pattern?pattern=gts.vendor.*&candidate=gts.vendor.package.namespace.type.v1.0~")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Pattern should match
    assert_eq!(result["match"], true);
}

#[tokio::test]
async fn test_uuid_endpoint() {
    let ops = create_test_ops();
    let app = create_test_router(ops, 0);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/uuid?gts_id=gts.vendor:package:schema~")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(result["uuid"].is_string());
}

#[tokio::test]
async fn test_add_schema_endpoint() {
    let ops = create_test_ops();
    let app = create_test_router(ops, 0);

    let schema = serde_json::json!({
        "type_id": "test:schema:v1",
        "schema": {
            "$id": "gts://test:schema:v1",
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            }
        }
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/schemas")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&schema).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_add_entity_endpoint_without_id() {
    let ops = create_test_ops();
    let app = create_test_router(ops, 0);

    let entity = serde_json::json!({
        "name": "Test"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/entities?validate=false")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&entity).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 422 for invalid entity
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_get_entities_endpoint_default_limit() {
    let ops = create_test_ops();
    let app = create_test_router(ops, 0);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/entities")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_extract_id_endpoint() {
    let ops = create_test_ops();
    let app = create_test_router(ops, 0);

    let entity = serde_json::json!({
        "$id": "gts://test:schema:v1",
        "type": "object"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/extract-id")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&entity).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_entity_success() {
    let mut ops = create_test_ops();

    // Add a test entity first
    let test_entity = serde_json::json!({
        "$id": "gts:gts.test.foo.v1:test123",
        "type": "gts:gts.test.foo.v1~",
        "name": "Test Entity"
    });

    ops.add_entity(&test_entity, false);

    let app = create_test_router(ops, 0);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/entities/gts:gts.test.foo.v1:test123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_entity_not_found() {
    let ops = create_test_ops();
    let app = create_test_router(ops, 0);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/entities/gts:gts.test.foo.v1:nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_add_entities_bulk() {
    let ops = create_test_ops();
    let app = create_test_router(ops, 0);

    let entities = serde_json::json!([
        {
            "$id": "gts:gts.test.foo.v1:entity1",
            "type": "gts:gts.test.foo.v1~",
            "name": "Entity 1"
        },
        {
            "$id": "gts:gts.test.foo.v1:entity2",
            "type": "gts:gts.test.foo.v1~",
            "name": "Entity 2"
        }
    ]);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/entities/bulk")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&entities).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_validate_instance_endpoint() {
    let mut ops = create_test_ops();

    // Add entity and schema
    let test_entity = serde_json::json!({
        "$id": "gts:gts.test.foo.v1:test123",
        "type": "gts:gts.test.foo.v1~",
        "name": "Test Entity"
    });

    ops.add_entity(&test_entity, false);

    let app = create_test_router(ops, 0);

    let request_body = serde_json::json!({
        "instance_id": "gts:gts.test.foo.v1:test123"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/validate-instance")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&request_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_schema_graph_endpoint() {
    let ops = create_test_ops();
    let app = create_test_router(ops, 0);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/resolve-relationships?gts_id=gts:gts.test.foo.v1~")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_compatibility_endpoint() {
    let ops = create_test_ops();
    let app = create_test_router(ops, 0);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/compatibility?old_schema_id=gts:gts.test.foo.v1~&new_schema_id=gts:gts.test.foo.v2~")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_cast_endpoint() {
    let mut ops = create_test_ops();

    // Add test entity
    let test_entity = serde_json::json!({
        "$id": "gts:gts.test.foo.v1:test123",
        "type": "gts:gts.test.foo.v1~",
        "name": "Test Entity"
    });

    ops.add_entity(&test_entity, false);

    let app = create_test_router(ops, 0);

    let request_body = serde_json::json!({
        "instance_id": "gts:gts.test.foo.v1:test123",
        "to_schema_id": "gts:gts.test.foo.v2~"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/cast")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&request_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_query_endpoint() {
    let mut ops = create_test_ops();

    // Add test entity
    let test_entity = serde_json::json!({
        "$id": "gts:gts.test.foo.v1:test123",
        "type": "gts:gts.test.foo.v1~",
        "name": "Test Entity"
    });

    ops.add_entity(&test_entity, false);

    let app = create_test_router(ops, 0);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/query?expr=type%3Dgts%3Agts.test.foo.v1%7E&limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_attr_endpoint() {
    let mut ops = create_test_ops();

    // Add test entity
    let test_entity = serde_json::json!({
        "$id": "gts:gts.test.foo.v1:test123",
        "type": "gts:gts.test.foo.v1~",
        "name": "Test Entity"
    });

    ops.add_entity(&test_entity, false);

    let app = create_test_router(ops, 0);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/attr?gts_with_path=gts:gts.test.foo.v1:test123.name")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
