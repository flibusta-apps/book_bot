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
        .bearer_auth(&config::CONFIG.manager_api_key)
        .send()
        .await?;

    parse_bots_response(response).await
}

async fn parse_bots_response(response: reqwest::Response) -> Result<Vec<BotData>, reqwest::Error> {
    response.error_for_status()?.json::<Vec<BotData>>().await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_with_status(status: u16) -> reqwest::Response {
        let http_response = http::Response::builder()
            .status(status)
            .body(Vec::<u8>::new())
            .unwrap();
        reqwest::Response::from(http_response)
    }

    #[tokio::test]
    async fn parse_bots_response_errors_on_401() {
        let response = response_with_status(401);
        assert!(parse_bots_response(response).await.is_err());
    }

    #[tokio::test]
    async fn parse_bots_response_parses_valid_json() {
        let http_response = http::Response::builder()
            .status(200)
            .body(br#"[{"id":1,"token":"abc","cache":"cache"}]"#.to_vec())
            .unwrap();
        let response = reqwest::Response::from(http_response);

        let bots = parse_bots_response(response).await.unwrap();
        assert_eq!(bots.len(), 1);
        assert_eq!(bots[0].id, 1);
        assert_eq!(bots[0].cache, BotCache::Cache);
    }
}
