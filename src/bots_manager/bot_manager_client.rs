use serde::Deserialize;

use crate::config;

#[derive(Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum BotCache {
    #[serde(rename = "original")]
    Original,
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
    let client = reqwest::Client::new();
    let response = client
        .get(&config::CONFIG.manager_url)
        .header("Authorization", &config::CONFIG.manager_api_key)
        .send()
        .await;

    match response {
        Ok(v) => v.json::<Vec<BotData>>().await,
        Err(err) => Err(err),
    }
}
