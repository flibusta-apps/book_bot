use smallvec::SmallVec;
use smartstring::alias::String as SmartString;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::config;

pub static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client")
});

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskObjectType {
    Sequence,
    Author,
    Translator,
}

#[derive(Deserialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    InProgress,
    Archiving,
    Complete,
    Failed,
}

#[derive(Serialize)]
pub struct CreateTaskData {
    pub object_id: u32,
    pub object_type: TaskObjectType,
    pub file_format: String,
    pub allowed_langs: SmallVec<[SmartString; 3]>,
    /// When `true` (the default), archive members have transliterated
    /// (GOST 7.79B) names. Set to `false` to keep Cyrillic names.
    /// Mirrors the cache server's `?normalized=` parameter.
    #[serde(default = "default_normalized_true")]
    pub normalized: bool,
}

#[allow(dead_code)] // referenced by `#[serde(default = ...)]` above
fn default_normalized_true() -> bool {
    true
}

#[derive(Deserialize, Clone)]
pub struct Task {
    pub id: String,
    pub status: TaskStatus,
    pub status_description: String,
    pub error_message: Option<String>,
    pub result_filename: Option<String>,
    pub content_size: Option<u64>,
}

pub async fn create_task(data: CreateTaskData, user_id: Option<u64>) -> anyhow::Result<Task> {
    let mut request = CLIENT
        .post(format!("{}/api/", &config::CONFIG.batch_downloader_url))
        .body(serde_json::to_string(&data).unwrap())
        .header("Authorization", &config::CONFIG.batch_downloader_api_key)
        .header("Content-Type", "application/json");

    if let Some(uid) = user_id {
        request = request.header("X-User-Id", uid.to_string());
    }

    Ok(request
        .send()
        .await?
        .error_for_status()?
        .json::<Task>()
        .await?)
}

pub async fn get_task(task_id: &str) -> anyhow::Result<Task> {
    Ok(CLIENT
        .get(format!(
            "{}/api/check_archive/{task_id}",
            &config::CONFIG.batch_downloader_url
        ))
        .header("Authorization", &config::CONFIG.batch_downloader_api_key)
        .send()
        .await?
        .error_for_status()?
        .json::<Task>()
        .await?)
}
