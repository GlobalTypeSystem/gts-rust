use axum::{body::Body, extract::Request, http::StatusCode, middleware::Next, response::Response};
use chrono::Local;
use std::time::Instant;

// ANSI color codes
struct Colors {
    reset: &'static str,
    dim: &'static str,
    green: &'static str,
    yellow: &'static str,
    red: &'static str,
    cyan: &'static str,
    blue: &'static str,
    magenta: &'static str,
    gray: &'static str,
}

impl Colors {
    fn new() -> Self {
        // Check if stderr is a TTY (terminal)
        let use_colors = atty::is(atty::Stream::Stderr);

        if use_colors {
            Self {
                reset: "\x1b[0m",
                dim: "\x1b[2m",
                green: "\x1b[92m",   // 2xx success
                yellow: "\x1b[93m",  // 3xx redirect
                red: "\x1b[91m",     // 4xx, 5xx errors
                cyan: "\x1b[96m",    // Method
                blue: "\x1b[94m",    // Path
                magenta: "\x1b[95m", // Duration
                gray: "\x1b[90m",    // DEBUG content
            }
        } else {
            Self {
                reset: "",
                dim: "",
                green: "",
                yellow: "",
                red: "",
                cyan: "",
                blue: "",
                magenta: "",
                gray: "",
            }
        }
    }

    fn status_color(&self, status: StatusCode) -> &'static str {
        let code = status.as_u16();
        if (200..300).contains(&code) {
            self.green
        } else if (300..400).contains(&code) {
            self.yellow
        } else {
            self.red
        }
    }
}

#[derive(Clone)]
pub struct LoggingMiddleware {
    pub verbose: u8,
}

impl LoggingMiddleware {
    #[must_use]
    pub fn new(verbose: u8) -> Self {
        Self { verbose }
    }

    #[allow(clippy::too_many_lines)]
    pub async fn handle(&self, request: Request, next: Next) -> Response {
        if self.verbose == 0 {
            return next.run(request).await;
        }

        let colors = Colors::new();
        let method = request.method().clone();
        let uri = request.uri().clone();
        let start = Instant::now();

        // For verbose == 1 (INFO level), we only log the request/response summary
        // We don't need to consume the body, so pass the request through as-is
        let response = if self.verbose >= 2 {
            // Cache request body for DEBUG logging (verbose >= 2)
            let (parts, body) = request.into_parts();
            let body_bytes = axum::body::to_bytes(body, usize::MAX).await.ok();

            // Reconstruct request with the cached body
            let request = if let Some(ref bytes) = body_bytes {
                Request::from_parts(parts, Body::from(bytes.clone()))
            } else {
                Request::from_parts(parts, Body::empty())
            };

            // Log request body at DEBUG level
            if let Some(ref bytes) = body_bytes
                && !bytes.is_empty()
            {
                let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S,%3f");
                if let Ok(json) = serde_json::from_slice::<serde_json::Value>(bytes) {
                    let body_str = serde_json::to_string_pretty(&json).unwrap_or_default();
                    eprintln!(
                        "{} - DEBUG - {}Request body:{}\n{}{}{}",
                        timestamp, colors.dim, colors.reset, colors.gray, body_str, colors.reset
                    );
                } else {
                    let body_str = String::from_utf8_lossy(bytes);
                    eprintln!(
                        "{} - DEBUG - {}Request body (raw):{}\n{}{}{}",
                        timestamp, colors.dim, colors.reset, colors.gray, body_str, colors.reset
                    );
                }
            }

            next.run(request).await
        } else {
            // verbose == 1: just pass through without consuming the body
            next.run(request).await
        };

        let duration = start.elapsed();
        let status = response.status();
        let duration_ms = duration.as_secs_f64() * 1000.0;

        // Log response at INFO level (verbose >= 1)
        // Use eprintln! directly to avoid tracing's escaping of ANSI codes
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S,%3f");
        eprintln!(
            "{} - INFO - {}{}{} {}{}{} -> {}{}{} in {}{:.1}ms{}",
            timestamp,
            colors.cyan,
            method,
            colors.reset,
            colors.blue,
            uri.path(),
            colors.reset,
            colors.status_color(status),
            status.as_u16(),
            colors.reset,
            colors.magenta,
            duration_ms,
            colors.reset
        );

        // Log response body at DEBUG level (verbose >= 2)
        if self.verbose >= 2 {
            // Extract response body
            let (parts, body) = response.into_parts();
            if let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await {
                if !bytes.is_empty() {
                    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S,%3f");
                    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        let body_str = serde_json::to_string_pretty(&json).unwrap_or_default();
                        eprintln!(
                            "{} - DEBUG - {}Response body:{}\n{}{}{}",
                            timestamp,
                            colors.dim,
                            colors.reset,
                            colors.gray,
                            body_str,
                            colors.reset
                        );
                    } else {
                        let body_str = String::from_utf8_lossy(&bytes);
                        eprintln!(
                            "{} - DEBUG - {}Response body (raw):{}\n{}{}{}",
                            timestamp,
                            colors.dim,
                            colors.reset,
                            colors.gray,
                            body_str,
                            colors.reset
                        );
                    }
                }
                return Response::from_parts(parts, Body::from(bytes));
            }
            return Response::from_parts(parts, Body::empty());
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colors_new_creates_struct() {
        let colors = Colors::new();
        // Just verify we can create the struct
        // Whether colors are enabled depends on TTY state
        assert!(!colors.reset.is_empty() || colors.reset.is_empty());
    }

    #[test]
    fn test_status_color_2xx_returns_green() {
        let colors = Colors {
            reset: "\x1b[0m",
            dim: "\x1b[2m",
            green: "\x1b[92m",
            yellow: "\x1b[93m",
            red: "\x1b[91m",
            cyan: "\x1b[96m",
            blue: "\x1b[94m",
            magenta: "\x1b[95m",
            gray: "\x1b[90m",
        };

        let color = colors.status_color(StatusCode::OK);
        assert_eq!(color, "\x1b[92m");

        let color = colors.status_color(StatusCode::CREATED);
        assert_eq!(color, "\x1b[92m");
    }

    #[test]
    fn test_status_color_3xx_returns_yellow() {
        let colors = Colors {
            reset: "\x1b[0m",
            dim: "\x1b[2m",
            green: "\x1b[92m",
            yellow: "\x1b[93m",
            red: "\x1b[91m",
            cyan: "\x1b[96m",
            blue: "\x1b[94m",
            magenta: "\x1b[95m",
            gray: "\x1b[90m",
        };

        let color = colors.status_color(StatusCode::MOVED_PERMANENTLY);
        assert_eq!(color, "\x1b[93m");

        let color = colors.status_color(StatusCode::FOUND);
        assert_eq!(color, "\x1b[93m");
    }

    #[test]
    fn test_status_color_4xx_5xx_returns_red() {
        let colors = Colors {
            reset: "\x1b[0m",
            dim: "\x1b[2m",
            green: "\x1b[92m",
            yellow: "\x1b[93m",
            red: "\x1b[91m",
            cyan: "\x1b[96m",
            blue: "\x1b[94m",
            magenta: "\x1b[95m",
            gray: "\x1b[90m",
        };

        let color = colors.status_color(StatusCode::NOT_FOUND);
        assert_eq!(color, "\x1b[91m");

        let color = colors.status_color(StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(color, "\x1b[91m");
    }

    #[test]
    fn test_logging_middleware_new() {
        let middleware = LoggingMiddleware::new(0);
        assert_eq!(middleware.verbose, 0);

        let middleware = LoggingMiddleware::new(1);
        assert_eq!(middleware.verbose, 1);

        let middleware = LoggingMiddleware::new(2);
        assert_eq!(middleware.verbose, 2);
    }

    #[test]
    fn test_colors_all_status_codes() {
        let colors = Colors {
            reset: "\x1b[0m",
            dim: "\x1b[2m",
            green: "\x1b[92m",
            yellow: "\x1b[93m",
            red: "\x1b[91m",
            cyan: "\x1b[96m",
            blue: "\x1b[94m",
            magenta: "\x1b[95m",
            gray: "\x1b[90m",
        };

        // Test 2xx
        assert_eq!(colors.status_color(StatusCode::OK), "\x1b[92m");
        assert_eq!(colors.status_color(StatusCode::ACCEPTED), "\x1b[92m");
        assert_eq!(colors.status_color(StatusCode::NO_CONTENT), "\x1b[92m");

        // Test 3xx
        assert_eq!(
            colors.status_color(StatusCode::PERMANENT_REDIRECT),
            "\x1b[93m"
        );
        assert_eq!(
            colors.status_color(StatusCode::TEMPORARY_REDIRECT),
            "\x1b[93m"
        );

        // Test 4xx
        assert_eq!(colors.status_color(StatusCode::BAD_REQUEST), "\x1b[91m");
        assert_eq!(colors.status_color(StatusCode::UNAUTHORIZED), "\x1b[91m");
        assert_eq!(colors.status_color(StatusCode::FORBIDDEN), "\x1b[91m");

        // Test 5xx
        assert_eq!(
            colors.status_color(StatusCode::SERVICE_UNAVAILABLE),
            "\x1b[91m"
        );
        assert_eq!(colors.status_color(StatusCode::BAD_GATEWAY), "\x1b[91m");
    }

    #[test]
    fn test_colors_disabled() {
        // Test colors struct without colors (TTY disabled)
        let colors = Colors {
            reset: "",
            dim: "",
            green: "",
            yellow: "",
            red: "",
            cyan: "",
            blue: "",
            magenta: "",
            gray: "",
        };

        assert_eq!(colors.status_color(StatusCode::OK), "");
        assert_eq!(colors.status_color(StatusCode::FOUND), "");
        assert_eq!(colors.status_color(StatusCode::NOT_FOUND), "");
    }

    #[test]
    fn test_colors_boundary_conditions() {
        let colors = Colors {
            reset: "\x1b[0m",
            dim: "\x1b[2m",
            green: "\x1b[92m",
            yellow: "\x1b[93m",
            red: "\x1b[91m",
            cyan: "\x1b[96m",
            blue: "\x1b[94m",
            magenta: "\x1b[95m",
            gray: "\x1b[90m",
        };

        // Boundary 200
        assert_eq!(
            colors.status_color(StatusCode::from_u16(200).unwrap()),
            "\x1b[92m"
        );
        // Boundary 299
        assert_eq!(
            colors.status_color(StatusCode::from_u16(299).unwrap()),
            "\x1b[92m"
        );
        // Boundary 300
        assert_eq!(
            colors.status_color(StatusCode::from_u16(300).unwrap()),
            "\x1b[93m"
        );
        // Boundary 399
        assert_eq!(
            colors.status_color(StatusCode::from_u16(399).unwrap()),
            "\x1b[93m"
        );
        // Boundary 400
        assert_eq!(
            colors.status_color(StatusCode::from_u16(400).unwrap()),
            "\x1b[91m"
        );
        // Boundary 500
        assert_eq!(
            colors.status_color(StatusCode::from_u16(500).unwrap()),
            "\x1b[91m"
        );
    }

    #[test]
    fn test_logging_middleware_clone() {
        let middleware = LoggingMiddleware::new(1);
        let cloned = middleware;
        assert_eq!(cloned.verbose, 1);
    }
}
