use reqwest::header::RETRY_AFTER;
use serde::Deserialize;
use std::time::Duration;
use tokio::time::sleep;
use tracing::warn;

/// Which cache-server operation hit the rate limit.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheRateLimitOperation {
    #[serde(rename = "cache_hit")]
    Hit,
    #[serde(rename = "cache_hit_copy")]
    HitCopy,
    #[serde(rename = "cache_hit_download")]
    HitDownload,
    #[serde(rename = "cache_miss")]
    Miss,
}

impl std::fmt::Display for CacheRateLimitOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hit => write!(f, "cache_hit"),
            Self::HitCopy => write!(f, "cache_hit_copy"),
            Self::HitDownload => write!(f, "cache_hit_download"),
            Self::Miss => write!(f, "cache_miss"),
        }
    }
}

/// Body returned by the cache-server on `429 Too Many Requests`.
#[derive(Deserialize, Debug, Clone)]
pub struct RateLimitError {
    #[allow(dead_code)]
    pub error: String,
    pub operation: CacheRateLimitOperation,
    pub retry_after_secs: u64,
}

/// Parsed result of a 429 response — extracts wait duration and optional details.
pub struct RateLimitInfo {
    pub retry_after: Duration,
    pub operation: Option<CacheRateLimitOperation>,
}

/// Default wait when neither `Retry-After` header nor body `retry_after_secs` is present.
const DEFAULT_RETRY_AFTER_SECS: u64 = 5;

/// Maximum number of retry attempts for a rate-limited request.
const MAX_RETRIES: u32 = 3;

impl RateLimitInfo {
    /// Parse a 429 response: reads the body once, prefers `Retry-After` header.
    ///
    /// **Consumes** `response` because `reqwest::Response::text()` takes ownership.
    pub async fn from_response(response: reqwest::Response) -> Self {
        let from_header = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());

        // Read body once, try to parse as RateLimitError for operation details.
        let (body_secs, operation) = match response.text().await {
            Ok(text) => match serde_json::from_str::<RateLimitError>(&text) {
                Ok(rl) => (Some(rl.retry_after_secs), Some(rl.operation)),
                Err(err) => {
                    warn!("Failed to parse 429 body: {err}, body: {text}");
                    (None, None)
                }
            },
            Err(err) => {
                warn!("Failed to read 429 body: {err}");
                (None, None)
            }
        };

        let secs = from_header.unwrap_or(body_secs.unwrap_or(DEFAULT_RETRY_AFTER_SECS));

        Self {
            retry_after: Duration::from_secs(secs),
            operation,
        }
    }
}

/// Execute a fallible request with automatic 429 retry + exponential backoff.
///
/// * `has_user_id` — whether `X-User-Id` was sent. Anonymous requests (no
///   user ID) share a rate-limit slot — per spec they must **not** be retried
///   because retries only worsen congestion on the shared slot.
/// * `make_request` — closure that produces a fresh `reqwest::Response`
///   (called once per attempt so the body can be re-created).
/// * Returns the first successful response, or the last error.
pub async fn retry_on_429<F, Fut>(
    has_user_id: bool,
    make_request: F,
) -> anyhow::Result<reqwest::Response>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<reqwest::Response, reqwest::Error>>,
{
    let mut attempt: u32 = 0;
    let max_attempts = MAX_RETRIES + 1; // initial request + retries

    loop {
        let response = make_request().await?;

        if response.status() != reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Ok(response);
        }

        let info = RateLimitInfo::from_response(response).await;

        // Anonymous requests share a rate limit — don't retry.
        if !has_user_id {
            return Err(anyhow::anyhow!(
                "rate_limit_exceeded (anonymous): operation={}, retry_after={}s",
                info.operation
                    .map(|op| op.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                info.retry_after.as_secs(),
            ));
        }

        let backoff = Duration::from_secs(2u64.saturating_pow(attempt));
        let wait = info.retry_after.max(backoff);

        warn!(
            "Rate limited (attempt {}/{}, max), operation={:?}, waiting {}s",
            attempt + 1,
            max_attempts,
            info.operation,
            wait.as_secs(),
        );

        if attempt + 1 >= max_attempts {
            return Err(anyhow::anyhow!(
                "rate_limit_exceeded: operation={}, retry_after={}s",
                info.operation
                    .map(|op| op.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                info.retry_after.as_secs(),
            ));
        }

        sleep(wait).await;
        attempt += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_rate_limit_operation_display() {
        assert_eq!(CacheRateLimitOperation::Hit.to_string(), "cache_hit");
        assert_eq!(
            CacheRateLimitOperation::HitCopy.to_string(),
            "cache_hit_copy"
        );
        assert_eq!(
            CacheRateLimitOperation::HitDownload.to_string(),
            "cache_hit_download"
        );
        assert_eq!(CacheRateLimitOperation::Miss.to_string(), "cache_miss");
    }

    #[test]
    fn rate_limit_error_deserialization() {
        let json = r#"{"error":"rate_limit_exceeded","operation":"cache_hit_download","retry_after_secs":7}"#;
        let rl: RateLimitError = serde_json::from_str(json).unwrap();
        assert_eq!(rl.error, "rate_limit_exceeded");
        assert_eq!(rl.operation, CacheRateLimitOperation::HitDownload);
        assert_eq!(rl.retry_after_secs, 7);
    }

    #[test]
    fn rate_limit_error_all_operations() {
        for (json, expected) in [
            (
                r#"{"error":"e","operation":"cache_hit","retry_after_secs":1}"#,
                CacheRateLimitOperation::Hit,
            ),
            (
                r#"{"error":"e","operation":"cache_hit_copy","retry_after_secs":1}"#,
                CacheRateLimitOperation::HitCopy,
            ),
            (
                r#"{"error":"e","operation":"cache_hit_download","retry_after_secs":1}"#,
                CacheRateLimitOperation::HitDownload,
            ),
            (
                r#"{"error":"e","operation":"cache_miss","retry_after_secs":1}"#,
                CacheRateLimitOperation::Miss,
            ),
        ] {
            let rl: RateLimitError = serde_json::from_str(json).unwrap();
            assert_eq!(rl.operation, expected);
        }
    }

    #[test]
    fn default_retry_after_secs_constant() {
        assert_eq!(DEFAULT_RETRY_AFTER_SECS, 5);
    }

    #[test]
    fn max_retries_constant() {
        assert_eq!(MAX_RETRIES, 3);
    }
}
