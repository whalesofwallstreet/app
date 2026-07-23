use axum::{
    body::{to_bytes, Body},
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use xxhash_rust::xxh3::xxh3_64;

/// Intercepts the response, generates an `xxHash` of the body, and checks against
/// the `If-None-Match` request header. If they match, returns HTTP 304 Not Modified.
/// Always injects `ETag` and `Cache-Control` headers for successful responses.
pub async fn etag_middleware(req: Request, next: Next) -> Result<Response, StatusCode> {
    let if_none_match = req
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let res = next.run(req).await;

    // Only apply caching to 200 OK responses
    if res.status() != StatusCode::OK {
        return Ok(res);
    }

    let (mut parts, body) = res.into_parts();

    // Read the response body into memory to generate the hash
    let bytes = match to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let hash = xxh3_64(&bytes);
    // Wrap etag in quotes per HTTP spec
    let etag = format!("\"{:x}\"", hash);

    parts.headers.insert(
        header::ETAG,
        etag.parse()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );
    parts.headers.insert(
        header::CACHE_CONTROL,
        "public, max-age=3600"
            .parse()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );

    if let Some(inm) = if_none_match {
        if inm == etag {
            parts.status = StatusCode::NOT_MODIFIED;
            // Short-circuit the response with a 304 and empty body
            return Ok(Response::from_parts(parts, Body::empty()));
        }
    }

    Ok(Response::from_parts(parts, Body::from(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        http::{header, StatusCode},
        routing::get,
        Router,
    };
    use axum_test::TestServer;

    async fn test_handler() -> &'static str {
        "hello world"
    }

    #[tokio::test]
    async fn test_etag_middleware_generates_etag() {
        let app = Router::new()
            .route("/", get(test_handler))
            .layer(axum::middleware::from_fn(etag_middleware));

        let server = TestServer::new(app).unwrap();

        let res = server.get("/").await;
        res.assert_status(StatusCode::OK);

        let etag = res.header(header::ETAG);
        assert!(!etag.to_str().unwrap().is_empty());

        let cache_control = res.header(header::CACHE_CONTROL);
        assert_eq!(cache_control.to_str().unwrap(), "public, max-age=3600");
    }

    #[tokio::test]
    async fn test_etag_middleware_returns_304() {
        let app = Router::new()
            .route("/", get(test_handler))
            .layer(axum::middleware::from_fn(etag_middleware));

        let server = TestServer::new(app).unwrap();

        // First request to get the ETag
        let res1 = server.get("/").await;
        res1.assert_status(StatusCode::OK);
        let etag = res1.header(header::ETAG).to_str().unwrap().to_string();

        // Second request with If-None-Match
        let res2 = server
            .get("/")
            .add_header(header::IF_NONE_MATCH, etag)
            .await;
        res2.assert_status(StatusCode::NOT_MODIFIED);

        // Body should be empty
        assert_eq!(res2.as_bytes().len(), 0);
    }
}
