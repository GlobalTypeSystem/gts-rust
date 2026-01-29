use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{Method, StatusCode},
    middleware,
    response::IntoResponse,
    routing::{get, post},
};
use gts_cli::logging::LoggingMiddleware;
use tower::ServiceExt;

// Helper function to create a simple handler that returns a 200 OK
async fn ok_handler() -> impl IntoResponse {
    (StatusCode::OK, "Success")
}

// Helper function to create a handler that returns JSON
async fn json_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        r#"{"message":"Hello","value":42}"#,
    )
}

// Helper function to create a handler that echoes the request body
async fn echo_handler(body: String) -> impl IntoResponse {
    (StatusCode::OK, body)
}

// Helper function to create a handler that returns 404
async fn not_found_handler() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Not found")
}

// Helper function to create a handler that returns 500
async fn server_error_handler() -> impl IntoResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
}

#[tokio::test]
async fn test_handle_verbose_0_no_logging() {
    // Verbose level 0 should not log anything
    let middleware = LoggingMiddleware::new(0);
    let app = Router::new()
        .route("/test", get(ok_handler))
        .layer(middleware::from_fn(move |req, next| {
            let mw = middleware.clone();
            async move { mw.handle(req, next).await }
        }));

    let request = Request::builder()
        .method(Method::GET)
        .uri("/test")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    // No way to directly assert no logging, but the request should complete successfully
}

#[tokio::test]
async fn test_handle_verbose_1_info_logging_success() {
    // Verbose level 1 should log INFO level (request summary)
    let middleware = LoggingMiddleware::new(1);
    let app = Router::new()
        .route("/api/users", get(ok_handler))
        .layer(middleware::from_fn(move |req, next| {
            let mw = middleware.clone();
            async move { mw.handle(req, next).await }
        }));

    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/users")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    // Logging happens to stderr, should complete without errors
}

#[tokio::test]
async fn test_handle_verbose_1_info_logging_not_found() {
    // Test INFO logging with 404 status
    let middleware = LoggingMiddleware::new(1);
    let app = Router::new()
        .route("/not-found", get(not_found_handler))
        .layer(middleware::from_fn(move |req, next| {
            let mw = middleware.clone();
            async move { mw.handle(req, next).await }
        }));

    let request = Request::builder()
        .method(Method::GET)
        .uri("/not-found")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_handle_verbose_1_info_logging_server_error() {
    // Test INFO logging with 500 status
    let middleware = LoggingMiddleware::new(1);
    let app = Router::new()
        .route("/error", get(server_error_handler))
        .layer(middleware::from_fn(move |req, next| {
            let mw = middleware.clone();
            async move { mw.handle(req, next).await }
        }));

    let request = Request::builder()
        .method(Method::GET)
        .uri("/error")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_handle_verbose_2_debug_logging_with_json_request() {
    // Verbose level 2 should log DEBUG level (request and response bodies)
    let middleware = LoggingMiddleware::new(2);
    let app = Router::new()
        .route("/api/data", post(json_handler))
        .layer(middleware::from_fn(move |req, next| {
            let mw = middleware.clone();
            async move { mw.handle(req, next).await }
        }));

    let json_body = r#"{"name":"test","age":30}"#;
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/data")
        .header("content-type", "application/json")
        .body(Body::from(json_body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Read response body to verify it's intact
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8_lossy(&body_bytes);
    assert!(body_str.contains("Hello"));
    assert!(body_str.contains("42"));
}

#[tokio::test]
async fn test_handle_verbose_2_debug_logging_empty_body() {
    // Test DEBUG logging with empty request body
    let middleware = LoggingMiddleware::new(2);
    let app = Router::new()
        .route("/empty", get(ok_handler))
        .layer(middleware::from_fn(move |req, next| {
            let mw = middleware.clone();
            async move { mw.handle(req, next).await }
        }));

    let request = Request::builder()
        .method(Method::GET)
        .uri("/empty")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_handle_verbose_2_debug_logging_non_json_request() {
    // Test DEBUG logging with non-JSON request body
    let middleware = LoggingMiddleware::new(2);
    let app = Router::new()
        .route("/text", post(echo_handler))
        .layer(middleware::from_fn(move |req, next| {
            let mw = middleware.clone();
            async move { mw.handle(req, next).await }
        }));

    let text_body = "plain text data";
    let request = Request::builder()
        .method(Method::POST)
        .uri("/text")
        .header("content-type", "text/plain")
        .body(Body::from(text_body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify response body
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8_lossy(&body_bytes);
    assert_eq!(body_str, text_body);
}

#[tokio::test]
async fn test_handle_preserves_response_body_verbose_0() {
    // Ensure response body is preserved at verbose level 0
    let middleware = LoggingMiddleware::new(0);
    let app = Router::new()
        .route("/json", get(json_handler))
        .layer(middleware::from_fn(move |req, next| {
            let mw = middleware.clone();
            async move { mw.handle(req, next).await }
        }));

    let request = Request::builder()
        .method(Method::GET)
        .uri("/json")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8_lossy(&body_bytes);

    assert!(body_str.contains("message"));
    assert!(body_str.contains("Hello"));
}

#[tokio::test]
async fn test_handle_preserves_response_body_verbose_1() {
    // Ensure response body is preserved at verbose level 1
    let middleware = LoggingMiddleware::new(1);
    let app = Router::new()
        .route("/json", get(json_handler))
        .layer(middleware::from_fn(move |req, next| {
            let mw = middleware.clone();
            async move { mw.handle(req, next).await }
        }));

    let request = Request::builder()
        .method(Method::GET)
        .uri("/json")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8_lossy(&body_bytes);

    assert!(body_str.contains("message"));
    assert!(body_str.contains("Hello"));
}

#[tokio::test]
async fn test_handle_preserves_response_body_verbose_2() {
    // Ensure response body is preserved at verbose level 2
    let middleware = LoggingMiddleware::new(2);
    let app = Router::new()
        .route("/json", get(json_handler))
        .layer(middleware::from_fn(move |req, next| {
            let mw = middleware.clone();
            async move { mw.handle(req, next).await }
        }));

    let request = Request::builder()
        .method(Method::GET)
        .uri("/json")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8_lossy(&body_bytes);

    assert!(body_str.contains("message"));
    assert!(body_str.contains("Hello"));
}

#[tokio::test]
async fn test_handle_different_http_methods() {
    // Test that middleware works with different HTTP methods
    let middleware = LoggingMiddleware::new(1);

    // Test POST
    let app = Router::new()
        .route("/data", post(ok_handler))
        .layer(middleware::from_fn(move |req, next| {
            let mw = middleware.clone();
            async move { mw.handle(req, next).await }
        }));

    let request = Request::builder()
        .method(Method::POST)
        .uri("/data")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_handle_measures_duration() {
    // Test that the middleware completes and measures duration
    // We can't directly assert the logged duration, but we can verify the request completes
    let middleware = LoggingMiddleware::new(1);
    let app = Router::new()
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                (StatusCode::OK, "Done")
            }),
        )
        .layer(middleware::from_fn(move |req, next| {
            let mw = middleware.clone();
            async move { mw.handle(req, next).await }
        }));

    let request = Request::builder()
        .method(Method::GET)
        .uri("/slow")
        .body(Body::empty())
        .unwrap();

    let start = std::time::Instant::now();
    let response = app.oneshot(request).await.unwrap();
    let elapsed = start.elapsed();

    assert_eq!(response.status(), StatusCode::OK);
    // Verify some time elapsed (at least 10ms)
    assert!(elapsed.as_millis() >= 10);
}

#[tokio::test]
async fn test_handle_with_query_parameters() {
    // Test logging with query parameters in the URI
    let middleware = LoggingMiddleware::new(1);
    let app = Router::new()
        .route("/search", get(ok_handler))
        .layer(middleware::from_fn(move |req, next| {
            let mw = middleware.clone();
            async move { mw.handle(req, next).await }
        }));

    let request = Request::builder()
        .method(Method::GET)
        .uri("/search?q=test&limit=10")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_handle_response_status_codes() {
    // Test various response status codes
    let test_cases = vec![
        (StatusCode::OK, 200),
        (StatusCode::CREATED, 201),
        (StatusCode::ACCEPTED, 202),
        (StatusCode::NO_CONTENT, 204),
        (StatusCode::MOVED_PERMANENTLY, 301),
        (StatusCode::FOUND, 302),
        (StatusCode::NOT_MODIFIED, 304),
        (StatusCode::BAD_REQUEST, 400),
        (StatusCode::UNAUTHORIZED, 401),
        (StatusCode::FORBIDDEN, 403),
        (StatusCode::NOT_FOUND, 404),
        (StatusCode::INTERNAL_SERVER_ERROR, 500),
        (StatusCode::SERVICE_UNAVAILABLE, 503),
    ];

    for (status_code, expected_code) in test_cases {
        let middleware = LoggingMiddleware::new(1);
        let app = Router::new()
            .route(
                "/status",
                get(move || async move { (status_code, "Response") }),
            )
            .layer(middleware::from_fn(move |req, next| {
                let mw = middleware.clone();
                async move { mw.handle(req, next).await }
            }));

        let request = Request::builder()
            .method(Method::GET)
            .uri("/status")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status().as_u16(), expected_code);
    }
}
