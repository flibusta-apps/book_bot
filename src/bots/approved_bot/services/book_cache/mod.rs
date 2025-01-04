use base64::{engine::general_purpose, Engine};
use once_cell::sync::Lazy;
use reqwest::StatusCode;

use crate::{
    bots::approved_bot::modules::download::callback_data::DownloadQueryData,
    bots_manager::BotCache, config,
};

use self::types::{CachedMessage, DownloadFile};

pub mod types;

pub static CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

pub async fn get_cached_message(
    download_data: &DownloadQueryData,
    bot_cache: BotCache,
) -> Result<Option<CachedMessage>, Box<dyn std::error::Error + Send + Sync>> {
    let DownloadQueryData::DownloadData {
        book_id: id,
        file_type: format,
    } = download_data;

    let is_need_copy = bot_cache == BotCache::Cache;

    let response = CLIENT
        .get(format!(
            "{}/api/v1/{id}/{format}/?copy={is_need_copy}",
            &config::CONFIG.cache_server_url
        ))
        .header("Authorization", &config::CONFIG.cache_server_api_key)
        .send()
        .await?
        .error_for_status()?;

    if response.status() == StatusCode::NO_CONTENT {
        return Ok(None);
    };

    Ok(Some(response.json::<CachedMessage>().await?))
}

pub async fn download_file(
    download_data: &DownloadQueryData,
) -> Result<Option<DownloadFile>, Box<dyn std::error::Error + Send + Sync>> {
    let DownloadQueryData::DownloadData {
        book_id: id,
        file_type: format,
    } = download_data;

    let response = CLIENT
        .get(format!(
            "{}/api/v1/download/{id}/{format}/",
            &config::CONFIG.cache_server_url
        ))
        .header("Authorization", &config::CONFIG.cache_server_api_key)
        .send()
        .await?
        .error_for_status()?;

    if response.status() == StatusCode::NO_CONTENT {
        return Ok(None);
    };

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
    filename: String,
    link: String,
) -> Result<Option<DownloadFile>, Box<dyn std::error::Error + Send + Sync>> {
    let response = CLIENT.get(link).send().await?;

    if response.status() != StatusCode::OK {
        return Ok(None);
    };

    Ok(Some(DownloadFile {
        response,
        filename,
        caption: "".to_string(),
    }))
}
