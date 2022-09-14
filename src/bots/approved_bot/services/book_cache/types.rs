use serde::Deserialize;


#[derive(Deserialize, Debug, Clone)]
pub struct CachedMessageData {
    pub message_id: i32,
    pub chat_id: i64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CachedMessage {
    pub data: CachedMessageData,
}

pub struct DownloadFile {
    pub response: reqwest::Response,
    pub filename: String,
    pub caption: String,
}
