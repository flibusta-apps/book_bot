use compact_str::CompactString as SmartString;
use smallvec::SmallVec;

use serde::{Deserialize, Serialize};

use crate::{
    bots::approved_bot::services::{build_url, check_response, HTTP_CLIENT},
    config,
};

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
    /// When `true`, archive members have transliterated (GOST 7.79B) names.
    /// Set to `false` to keep Cyrillic names. Mirrors the cache server's
    /// `?normalized=` parameter.
    pub normalized: bool,
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

const DEFAULT_QUEUE_FULL_RETRY_SECS: u64 = 30;

/// batch-downloader's task queue is full (`503` + `Retry-After`).
#[derive(Debug)]
pub struct QueueFull {
    pub retry_after_secs: u64,
}

impl std::fmt::Display for QueueFull {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "batch-downloader queue is full, retry after {} s",
            self.retry_after_secs
        )
    }
}

impl std::error::Error for QueueFull {}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> u64 {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_QUEUE_FULL_RETRY_SECS)
}

pub async fn create_task(data: CreateTaskData, user_id: Option<u64>) -> anyhow::Result<Task> {
    let url = build_url(&config::CONFIG.batch_downloader_url, ["api", ""])?;

    let mut request = HTTP_CLIENT
        .post(url)
        .json(&data)
        .header("Authorization", &config::CONFIG.batch_downloader_api_key);

    if let Some(uid) = user_id {
        request = request.header("X-User-Id", uid.to_string());
    }

    let response = request.send().await?;

    if response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE {
        let retry_after_secs = parse_retry_after(response.headers());
        return Err(QueueFull { retry_after_secs }.into());
    }

    check_response(response, &[])
        .await?
        .ok_or_else(|| anyhow::anyhow!("batch-downloader service returned an empty response"))
}

pub async fn get_task(task_id: &str) -> anyhow::Result<Task> {
    let url = build_url(
        &config::CONFIG.batch_downloader_url,
        ["api", "check_archive", task_id],
    )?;

    let response = HTTP_CLIENT
        .get(url)
        .header("Authorization", &config::CONFIG.batch_downloader_api_key)
        .send()
        .await?;

    check_response(response, &[])
        .await?
        .ok_or_else(|| anyhow::anyhow!("batch-downloader service returned an empty response"))
}
