use reqwest::ClientBuilder;
use reqwest_middleware::{ClientBuilder as MiddlewareBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use std::time::Duration;

pub fn build_resilient_client() -> Result<ClientWithMiddleware, reqwest::Error> {
    // 1. Configure the underlying reqwest::Client
    // - Global timeout for the entire request
    // - Connect timeout for establishing the TCP connection
    let reqwest_client = ClientBuilder::new()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()?;

    // 2. Configure the retry policy
    // - Exponential backoff with up to 3 retries
    // - Automatically retries on 5xx errors and network timeouts
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);

    // 3. Wrap the client with the middleware
    let client = MiddlewareBuilder::new(reqwest_client)
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build();

    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::any;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_resilient_client_retries_on_500() {
        // Start a mock server
        let mock_server = MockServer::start().await;

        // Configure the mock server to ALWAYS return 500 Internal Server Error
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(4) // 1 initial request + 3 retries
            .mount(&mock_server)
            .await;

        let client = build_resilient_client().expect("Failed to build client");

        // The request should eventually complete with a 500 status after retrying
        let response = client
            .get(mock_server.uri())
            .send()
            .await
            .expect("Request failed");

        assert_eq!(
            response.status(),
            500,
            "Expected a 500 response after all retries were exhausted"
        );
    }

    #[tokio::test]
    async fn test_resilient_client_success() {
        let mock_server = MockServer::start().await;

        Mock::given(any())
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = build_resilient_client().expect("Failed to build client");

        let response = client
            .get(mock_server.uri())
            .send()
            .await
            .expect("Request failed");
        assert_eq!(response.status(), 200);
    }
}
