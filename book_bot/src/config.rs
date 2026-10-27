use std::sync::LazyLock;

pub struct Config {
    pub telegram_bot_api: reqwest::Url,

    pub webhook_base_url: String,
    pub webhook_port: u16,
    pub webhook_secret_token: String,

    // pub admin_id: String,
    // pub bot_token: String,
    pub manager_url: String,
    pub manager_api_key: String,

    // `user_settings_url`, `book_server_url`, `cache_server_url`,
    // `batch_downloader_url`, and `public_batch_downloader_url` are appended
    // to via `services::build_url`'s `path_segments_mut().extend(...)`,
    // which appends onto whatever path the base URL already has rather than
    // replacing it. These must be scheme+host+port only (no path component)
    // or the appended API paths will be nested under it.
    pub user_settings_url: reqwest::Url,
    pub user_settings_api_key: String,

    pub book_server_url: reqwest::Url,
    pub book_server_api_key: String,

    pub cache_server_url: reqwest::Url,
    pub cache_server_api_key: String,

    pub batch_downloader_url: reqwest::Url,
    pub public_batch_downloader_url: reqwest::Url,
    pub batch_downloader_api_key: String,

    pub sentry_dsn: Option<String>,
}

/// Strips an optional leading `Bearer ` (case-insensitive) from an API key.
pub fn normalize_api_key(raw: &str) -> String {
    let trimmed = raw.trim();
    match trimmed.get(..7) {
        Some(prefix) if prefix.eq_ignore_ascii_case("bearer ") => trimmed[7..].trim().to_string(),
        _ => trimmed.to_string(),
    }
}

fn get_non_empty_env(env: &'static str) -> String {
    let value = get_env(env);
    if value.trim().is_empty() {
        panic!("The {env} env variable must not be empty");
    }
    value
}

/// Webhook secrets must match Telegram's `secret_token` rules:
/// 1-256 characters, only `A-Z`, `a-z`, `0-9`, `_` and `-`.
fn validate_webhook_secret(secret: &str) -> bool {
    !secret.is_empty()
        && secret.len() <= 256
        && secret
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn get_env(env: &'static str) -> String {
    std::env::var(env).unwrap_or_else(|_| panic!("Cannot get the {env} env variable"))
}

impl Config {
    pub fn load() -> Config {
        Config {
            telegram_bot_api: reqwest::Url::parse(&get_env("TELEGRAM_BOT_API_ROOT"))
                .unwrap_or_else(|_| {
                    panic!("Cannot parse url from TELEGRAM_BOT_API_ROOT env variable")
                }),

            webhook_base_url: get_env("WEBHOOK_BASE_URL"),
            webhook_port: get_env("WEBHOOK_PORT")
                .parse()
                .unwrap_or_else(|_| panic!("Cannot parse WEBHOOK_PORT")),
            webhook_secret_token: {
                let secret = get_env("WEBHOOK_SECRET_TOKEN");
                if !validate_webhook_secret(&secret) {
                    panic!("WEBHOOK_SECRET_TOKEN must be 1-256 chars of A-Z, a-z, 0-9, '_' or '-'");
                }
                secret
            },

            manager_url: get_env("MANAGER_URL"),
            manager_api_key: {
                let key = normalize_api_key(&get_non_empty_env("MANAGER_API_KEY"));
                if key.is_empty() {
                    panic!("The MANAGER_API_KEY env variable must not be empty");
                }
                key
            },

            user_settings_url: reqwest::Url::parse(&get_env("USER_SETTINGS_URL"))
                .unwrap_or_else(|_| panic!("Cannot parse url from USER_SETTINGS_URL env variable")),
            user_settings_api_key: get_non_empty_env("USER_SETTINGS_API_KEY"),

            book_server_url: reqwest::Url::parse(&get_env("BOOK_SERVER_URL"))
                .unwrap_or_else(|_| panic!("Cannot parse url from BOOK_SERVER_URL env variable")),
            book_server_api_key: get_non_empty_env("BOOK_SERVER_API_KEY"),

            cache_server_url: reqwest::Url::parse(&get_env("CACHE_SERVER_URL"))
                .unwrap_or_else(|_| panic!("Cannot parse url from CACHE_SERVER_URL env variable")),
            cache_server_api_key: get_non_empty_env("CACHE_SERVER_API_KEY"),

            batch_downloader_url: reqwest::Url::parse(&get_env("BATCH_DOWNLOADER_URL"))
                .unwrap_or_else(|_| {
                    panic!("Cannot parse url from BATCH_DOWNLOADER_URL env variable")
                }),
            public_batch_downloader_url: reqwest::Url::parse(&get_env(
                "PUBLIC_BATCH_DOWNLOADER_URL",
            ))
            .unwrap_or_else(|_| {
                panic!("Cannot parse url from PUBLIC_BATCH_DOWNLOADER_URL env variable")
            }),
            batch_downloader_api_key: get_non_empty_env("BATCH_DOWNLOADER_API_KEY"),

            sentry_dsn: std::env::var("SENTRY_DSN").ok(),
        }
    }
}

pub static CONFIG: LazyLock<Config> = LazyLock::new(Config::load);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_bearer_prefix() {
        assert_eq!(normalize_api_key("Bearer abc"), "abc");
        assert_eq!(normalize_api_key("bearer  abc "), "abc");
        assert_eq!(normalize_api_key("abc"), "abc");
    }

    #[test]
    fn validates_webhook_secret() {
        assert!(validate_webhook_secret("abc_DEF-123"));
        assert!(!validate_webhook_secret(""));
        assert!(!validate_webhook_secret("bad secret"));
        assert!(!validate_webhook_secret(&"a".repeat(257)));
    }
}
