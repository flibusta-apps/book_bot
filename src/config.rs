use once_cell::sync::Lazy;

pub struct Config {
    pub telegram_bot_api: reqwest::Url,

    pub webhook_base_url: String,
    pub webhook_port: u16,

    // pub admin_id: String,
    // pub bot_token: String,
    pub manager_url: String,
    pub manager_api_key: String,

    pub user_settings_url: String,
    pub user_settings_api_key: String,

    pub book_server_url: String,
    pub book_server_api_key: String,

    pub cache_server_url: String,
    pub cache_server_api_key: String,

    pub batch_downloader_url: String,
    pub public_batch_downloader_url: String,
    pub batch_downloader_api_key: String,

    pub sentry_dsn: String,
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
            webhook_port: get_env("WEBHOOK_PORT").parse().unwrap(),

            manager_url: get_env("MANAGER_URL"),
            manager_api_key: get_env("MANAGER_API_KEY"),

            user_settings_url: get_env("USER_SETTINGS_URL"),
            user_settings_api_key: get_env("USER_SETTINGS_API_KEY"),

            book_server_url: get_env("BOOK_SERVER_URL"),
            book_server_api_key: get_env("BOOK_SERVER_API_KEY"),

            cache_server_url: get_env("CACHE_SERVER_URL"),
            cache_server_api_key: get_env("CACHE_SERVER_API_KEY"),

            batch_downloader_url: get_env("BATCH_DOWNLOADER_URL"),
            public_batch_downloader_url: get_env("PUBLIC_BATCH_DOWNLOADER_URL"),
            batch_downloader_api_key: get_env("BATCH_DOWNLOADER_API_KEY"),

            sentry_dsn: get_env("SENTRY_DSN"),
        }
    }
}

pub static CONFIG: Lazy<Config> = Lazy::new(Config::load);
