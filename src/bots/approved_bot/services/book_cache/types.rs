use serde::Deserialize;


#[derive(Deserialize, Debug, Clone)]
pub struct CachedMessage {
    pub message_id: i32,
    pub chat_id: i64,
}

pub struct DownloadFile {
    pub response: reqwest::Response,
    pub filename: String,
    pub caption: String,
}

#[derive(Deserialize)]
pub struct DownloadLink {
    pub link: String
}
