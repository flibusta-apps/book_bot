use crate::{bots::approved_bot::modules::download::DownloadData, config};

use self::types::{CachedMessage, DownloadFile};

pub mod types;

pub async fn get_cached_message(
    download_data: &DownloadData,
) -> Result<CachedMessage, Box<dyn std::error::Error + Send + Sync>> {
    let DownloadData { format, id } = download_data;

    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "{}/api/v1/{id}/{format}",
            &config::CONFIG.cache_server_url
        ))
        .header("Authorization", &config::CONFIG.cache_server_api_key)
        .send()
        .await;

    let response = match response {
        Ok(v) => v,
        Err(err) => return Err(Box::new(err)),
    };

    let response = match response.error_for_status() {
        Ok(v) => v,
        Err(err) => return Err(Box::new(err)),
    };

    match response.json::<CachedMessage>().await {
        Ok(v) => Ok(v),
        Err(err) => Err(Box::new(err)),
    }
}

pub async fn clear_book_cache(
    download_data: &DownloadData,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let DownloadData { format, id } = download_data;

    let client = reqwest::Client::new();
    let response = client
        .delete(format!(
            "{}/api/v1/{id}/{format}",
            &config::CONFIG.cache_server_url
        ))
        .header("Authorization", &config::CONFIG.cache_server_api_key)
        .send()
        .await;

    let response = match response {
        Ok(v) => v,
        Err(err) => return Err(Box::new(err)),
    };

    match response.error_for_status() {
        Ok(_) => Ok(()),
        Err(err) => return Err(Box::new(err)),
    }
}

pub async fn download_file(
    download_data: &DownloadData,
) -> Result<DownloadFile, Box<dyn std::error::Error + Send + Sync>> {
    let DownloadData { format, id } = download_data;

    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "{}/api/v1/download/{id}/{format}",
            &config::CONFIG.cache_server_url
        ))
        .header("Authorization", &config::CONFIG.cache_server_api_key)
        .send()
        .await;

    let response = match response {
        Ok(v) => v,
        Err(err) => return Err(Box::new(err)),
    };

    match response.error_for_status() {
        Ok(response) => {
            let headers = response.headers();
            let filename = headers
                .get("content-disposition")
                .unwrap()
                .to_str()
                .unwrap()
                .replace('"', "")
                .split("filename=")
                .collect::<Vec<&str>>()
                .get(1)
                .unwrap()
                .to_string();
            let caption = std::str::from_utf8(
                &base64::decode(headers.get("x-caption-b64").unwrap()).unwrap(),
            )
            .unwrap()
            .to_string();

            Ok(DownloadFile {
                response,
                filename,
                caption,
            })
        }
        Err(err) => return Err(Box::new(err)),
    }
}
