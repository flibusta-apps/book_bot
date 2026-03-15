use serde::Deserialize;
use std::sync::LazyLock;

use crate::config;

pub static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client")
});

#[derive(Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum BotCache {
    #[serde(rename = "original")]
    Original,
    #[serde(rename = "cache")]
    Cache,
    #[serde(rename = "no_cache")]
    NoCache,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BotData {
    pub id: u32,
    pub token: String,
    pub cache: BotCache,
}

pub async fn get_bots() -> Result<Vec<BotData>, reqwest::Error> {
    let response = CLIENT
        .get(&config::CONFIG.manager_url)
        .header("Authorization", &config::CONFIG.manager_api_key)
        .send()
        .await;

    match response {
        Ok(v) => v.json::<Vec<BotData>>().await,
        Err(err) => Err(err),
    }
}

pub async fn delete_bot(id: u32) -> Result<(), reqwest::Error> {
    let response = CLIENT
        .delete(format!("{}/{}/", config::CONFIG.manager_url, id))
        .header("Authorization", &config::CONFIG.manager_api_key)
        .send()
        .await;

    match response {
        Ok(_) => Ok(()),
        Err(err) => Err(err),
    }
}
