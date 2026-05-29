use base64::{engine::general_purpose, Engine};
use reqwest::StatusCode;
use std::sync::LazyLock;

use crate::{
    bots::approved_bot::modules::download::callback_data::DownloadQueryData,
    bots::approved_bot::services::rate_limit::retry_on_429, bots_manager::BotCache, config,
};

use self::types::{CachedMessage, DownloadFile};

pub mod types;

pub static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client")
});

pub async fn get_cached_message(
    download_data: &DownloadQueryData,
    bot_cache: BotCache,
    user_id: Option<u64>,
) -> anyhow::Result<Option<CachedMessage>> {
    let DownloadQueryData::DownloadData {
        book_id: id,
        file_type: format,
    } = download_data;

    let is_need_copy = bot_cache == BotCache::Cache;

    let response = retry_on_429(user_id.is_some(), || {
        let mut req = CLIENT
            .get(format!(
                "{}/api/v1/{id}/{format}/?copy={is_need_copy}",
                &config::CONFIG.cache_server_url
            ))
            .header("Authorization", &config::CONFIG.cache_server_api_key);

        if let Some(uid) = user_id {
            req = req.header("X-User-Id", uid.to_string());
        }

        req.send()
    })
    .await?;

    if response.status() == StatusCode::NO_CONTENT {
        return Ok(None);
    };

    let response = response.error_for_status()?;

    Ok(Some(response.json::<CachedMessage>().await?))
}

pub async fn download_file(
    download_data: &DownloadQueryData,
    user_id: Option<u64>,
) -> anyhow::Result<Option<DownloadFile>> {
    let DownloadQueryData::DownloadData {
        book_id: id,
        file_type: format,
    } = download_data;

    let response = retry_on_429(user_id.is_some(), || {
        let mut req = CLIENT
            .get(format!(
                "{}/api/v1/download/{id}/{format}/",
                &config::CONFIG.cache_server_url
            ))
            .header("Authorization", &config::CONFIG.cache_server_api_key);

        if let Some(uid) = user_id {
            req = req.header("X-User-Id", uid.to_string());
        }

        req.send()
    })
    .await?;

    if response.status() == StatusCode::NO_CONTENT {
        return Ok(None);
    };

    let response = response.error_for_status()?;

    let headers = response.headers();

    let base64_encoder = general_purpose::STANDARD;

    let filename = std::str::from_utf8(
        &base64_encoder
            .decode(headers.get("x-filename-b64").unwrap())
            .unwrap(),
    )
    .unwrap()
    .to_string();

    let caption = std::str::from_utf8(
        &base64_encoder
            .decode(headers.get("x-caption-b64").unwrap())
            .unwrap(),
    )
    .unwrap()
    .to_string();

    Ok(Some(DownloadFile {
        response,
        filename,
        caption,
    }))
}

pub async fn download_file_by_link(
    filename: &str,
    link: String,
) -> anyhow::Result<Option<DownloadFile>> {
    let response = CLIENT.get(link).send().await?;

    if response.status() != StatusCode::OK {
        return Ok(None);
    };

    Ok(Some(DownloadFile {
        response,
        filename: filename.to_string(),
        caption: "".to_string(),
    }))
}
