use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct CachedMessage {
    pub message_id: i32,
    pub chat_id: i64,
    /// Echo of the `?normalized=` query param the cache server used
    /// when creating this record. `None` if the server pre-dates this field.
    #[serde(default)]
    pub is_normalized: Option<bool>,
}

pub struct DownloadFile {
    pub response: reqwest::Response,
    pub filename: String,
    pub caption: String,
}
