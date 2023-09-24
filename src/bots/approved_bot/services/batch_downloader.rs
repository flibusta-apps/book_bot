use smallvec::SmallVec;
use smartstring::alias::String as SmartString;

use serde::{Deserialize, Serialize};

use crate::config;

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
}

#[derive(Deserialize, Clone)]
pub struct Task {
    pub id: String,
    pub status: TaskStatus,
    pub status_description: String,
    pub error_message: Option<String>,
    pub result_filename: Option<String>,
    pub result_link: Option<String>,
    pub content_size: Option<u64>,
}

pub async fn create_task(
    data: CreateTaskData,
) -> Result<Task, Box<dyn std::error::Error + Send + Sync>> {
    Ok(reqwest::Client::new()
        .post(format!("{}/api/", &config::CONFIG.batch_downloader_url))
        .body(serde_json::to_string(&data).unwrap())
        .header("Authorization", &config::CONFIG.batch_downloader_api_key)
        .header("Content-Type", "application/json")
        .send()
        .await?
        .error_for_status()?
        .json::<Task>()
        .await?)
}

pub async fn get_task(task_id: String) -> Result<Task, Box<dyn std::error::Error + Send + Sync>> {
    Ok(reqwest::Client::new()
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
