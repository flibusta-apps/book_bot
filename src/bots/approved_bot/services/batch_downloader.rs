use serde::{Deserialize, Serialize};

use crate::config;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskObjectType {
    Sequence,
    Author,
    Translator,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    InProgress,
    Archiving,
    Complete,
}

#[derive(Serialize)]
pub struct CreateTaskData {
    pub object_id: u32,
    pub object_type: TaskObjectType,
    pub file_format: String,
    pub allowed_langs: Vec<String>,
}

#[derive(Deserialize)]
pub struct Task {
    pub id: String,
    pub object_id: u32,
    pub object_type: TaskObjectType,
    pub status: TaskStatus,
    pub result_link: Option<String>,
}

pub async fn create_task(
    data: CreateTaskData,
) -> Result<Task, Box<dyn std::error::Error + Send + Sync>> {
    Ok(reqwest::Client::new()
        .post(format!("{}/api/", &config::CONFIG.batch_downloader_url))
        .body(serde_json::to_string(&data).unwrap())
        .header("Authorization", &config::CONFIG.batch_downloader_api_key)
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
