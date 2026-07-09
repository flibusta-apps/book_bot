pub mod batch_downloader;
pub mod book_cache;
pub mod book_library;
pub mod donation_notifications;
pub mod rate_limit;
pub mod user_settings;

use std::sync::LazyLock;
use std::time::Duration;

use reqwest::{StatusCode, Url};
use serde::de::DeserializeOwned;
use tracing::log;

pub static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(5))
        .pool_idle_timeout(Duration::from_secs(90))
        .user_agent(concat!("book_bot/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("Failed to create HTTP client")
});

/// Appends path segments to a base URL, letting the `url` crate handle
/// percent-encoding (e.g. a space becomes `%20`, never `+`).
pub fn build_url<'a>(
    base: &Url,
    segments: impl IntoIterator<Item = &'a str>,
) -> anyhow::Result<Url> {
    let mut url = base.clone();
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("URL has cannot-be-a-base scheme"))?
        .extend(segments);
    Ok(url)
}

/// Splits a response by status: any status in `empty_statuses` -> `Ok(None)`;
/// any other 4xx/5xx -> `Err` (status, URL, and a truncated response body
/// are logged); anything else -> `Ok(Some(response))`, with the body not
/// yet read so callers needing raw bytes/headers can still consume it.
pub async fn check_status(
    response: reqwest::Response,
    empty_statuses: &[StatusCode],
) -> anyhow::Result<Option<reqwest::Response>> {
    let status = response.status();

    if empty_statuses.contains(&status) {
        return Ok(None);
    }

    if status.is_client_error() || status.is_server_error() {
        let url = response.url().clone();
        let body = response.text().await.unwrap_or_default();
        let truncated: String = body.chars().take(500).collect();
        log::error!("HTTP {status} from {url}: {truncated}");
        return Err(anyhow::anyhow!("HTTP {status} from {url}"));
    }

    Ok(Some(response))
}

/// `check_status` plus JSON deserialization. A deserialization failure is
/// logged with the URL before being returned as `Err` (previously only
/// `book_library` logged this).
pub async fn check_response<T: DeserializeOwned>(
    response: reqwest::Response,
    empty_statuses: &[StatusCode],
) -> anyhow::Result<Option<T>> {
    let Some(response) = check_status(response, empty_statuses).await? else {
        return Ok(None);
    };

    let url = response.url().clone();
    match response.json::<T>().await {
        Ok(value) => Ok(Some(value)),
        Err(err) => {
            log::error!("Failed to deserialize response from {url}: {err:?}");
            Err(err.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_with_status_and_body(status: u16, body: &str) -> reqwest::Response {
        let http_response = http::Response::builder()
            .status(status)
            .body(body.as_bytes().to_vec())
            .unwrap();
        reqwest::Response::from(http_response)
    }

    #[test]
    fn build_url_percent_encodes_spaces_as_percent20() {
        let base = Url::parse("https://example.com").unwrap();
        let url = build_url(&base, ["api", "v1", "books", "search", "war and peace"]).unwrap();
        assert_eq!(
            url.as_str(),
            "https://example.com/api/v1/books/search/war%20and%20peace"
        );
    }

    #[test]
    fn build_url_percent_encodes_unicode_instead_of_leaving_it_raw() {
        let base = Url::parse("https://example.com").unwrap();
        let query = "война";
        let url = build_url(&base, ["api", "v1", "books", "search", query]).unwrap();
        assert!(
            !url.as_str().contains(query),
            "unicode must be percent-encoded, not left raw"
        );
        assert!(
            !url.as_str().contains('+'),
            "must not use query-string '+' encoding"
        );
    }

    #[tokio::test]
    async fn check_status_returns_none_for_empty_status() {
        let response = response_with_status_and_body(204, "");
        let result = check_status(response, &[StatusCode::NO_CONTENT])
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn check_status_errors_on_non_empty_error_status() {
        let response = response_with_status_and_body(500, "server exploded");
        let result = check_status(response, &[StatusCode::NO_CONTENT]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn check_status_passes_through_success() {
        let response = response_with_status_and_body(200, "ok");
        let result = check_status(response, &[StatusCode::NO_CONTENT])
            .await
            .unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn check_response_deserializes_success_body() {
        #[derive(serde::Deserialize)]
        struct Foo {
            a: u32,
        }

        let response = response_with_status_and_body(200, r#"{"a":1}"#);
        let result: Option<Foo> = check_response(response, &[StatusCode::NOT_FOUND])
            .await
            .unwrap();
        assert_eq!(result.unwrap().a, 1);
    }

    #[tokio::test]
    async fn check_response_returns_none_for_empty_status() {
        let response = response_with_status_and_body(404, "not found");
        let result: Option<serde_json::Value> = check_response(response, &[StatusCode::NOT_FOUND])
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn check_response_errors_on_bad_json() {
        let response = response_with_status_and_body(200, "not json");
        let result: anyhow::Result<Option<serde_json::Value>> = check_response(response, &[]).await;
        assert!(result.is_err());
    }
}
