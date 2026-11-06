use reqwest::StatusCode;
use tracing::log;

use crate::{
    bots::approved_bot::modules::download::callback_data::DownloadQueryData,
    bots::approved_bot::services::{
        build_url, check_response, check_status,
        rate_limit::retry_on_429,
        user_settings::{get_user_file_name_lang_for, FileNameLang},
        CACHE_HTTP_CLIENT, FILE_HTTP_CLIENT,
    },
    bots_manager::BotCache,
    config,
};

use self::types::{CachedMessage, DownloadFile};

pub mod types;

// TFCS used 204 before switching absent books/files to 404.
const ABSENT_STATUSES: &[StatusCode] = &[StatusCode::NOT_FOUND, StatusCode::NO_CONTENT];

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
    // The cache server now stores separate records per `normalized` mode.
    // Mirror the user's setting here so we hit the same record that
    // `download_file` would later request.
    let requested_original = matches!(
        get_user_file_name_lang_for(user_id).await,
        FileNameLang::Original
    );

    let mut url = build_url(
        &config::CONFIG.cache_server_url,
        ["api", "v1", &id.to_string(), format, ""],
    )?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("copy", &is_need_copy.to_string());
        if requested_original {
            q.append_pair("normalized", "false");
        }
    }

    let response = retry_on_429(user_id.is_some(), || {
        let mut req = CACHE_HTTP_CLIENT
            .get(url.clone())
            .header("Authorization", &config::CONFIG.cache_server_api_key);

        if let Some(uid) = user_id {
            req = req.header("X-User-Id", uid.to_string());
        }

        req.send()
    })
    .await?;

    let cached: Option<CachedMessage> = check_response(response, ABSENT_STATUSES).await?;

    // The server echoes back the mode it actually used. If it disagrees
    // with what we requested, we're talking to an older / misconfigured
    // server and subsequent `download_file` calls will miss the cache.
    if let Some(cached) = &cached {
        if let Some(echo) = cached.is_normalized {
            let echoed_original = !echo;
            if echoed_original != requested_original {
                log::warn!(
                    "cache server echoed is_normalized={echo} for {id}/{format} \
                     but client requested original={requested_original}; \
                     cache mode may be inconsistent"
                );
            }
        }
    }

    Ok(cached)
}

fn decode_b64_header(headers: &reqwest::header::HeaderMap, name: &str) -> anyhow::Result<String> {
    use anyhow::Context as _;
    use base64::{engine::general_purpose, Engine as _};

    let raw = headers
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("missing response header: {name}"))?;
    let decoded_bytes = general_purpose::STANDARD
        .decode(raw)
        .with_context(|| format!("invalid base64 in header {name}"))?;
    std::str::from_utf8(&decoded_bytes)
        .with_context(|| format!("non-UTF8 content in header {name}"))
        .map(|s| s.to_string())
}

pub async fn download_file(
    download_data: &DownloadQueryData,
    user_id: Option<u64>,
) -> anyhow::Result<Option<DownloadFile>> {
    let DownloadQueryData::DownloadData {
        book_id: id,
        file_type: format,
    } = download_data;

    // If the user has selected original (Cyrillic) file names, ask the
    // cache server not to transliterate. Default (Normalized / unknown)
    // matches the previous behavior — no query param is sent, the server
    // falls back to `normalized=true`.
    let original = matches!(
        get_user_file_name_lang_for(user_id).await,
        FileNameLang::Original
    );

    let mut url = build_url(
        &config::CONFIG.cache_server_url,
        ["api", "v1", "download", &id.to_string(), format, ""],
    )?;
    if original {
        url.query_pairs_mut().append_pair("normalized", "false");
    }

    let response = retry_on_429(user_id.is_some(), || {
        let mut req = CACHE_HTTP_CLIENT
            .get(url.clone())
            .header("Authorization", &config::CONFIG.cache_server_api_key);

        if let Some(uid) = user_id {
            req = req.header("X-User-Id", uid.to_string());
        }

        req.send()
    })
    .await?;

    let Some(response) = check_status(response, ABSENT_STATUSES).await? else {
        return Ok(None);
    };

    let headers = response.headers();

    let filename = decode_b64_header(headers, "x-filename-b64")?;
    let caption = decode_b64_header(headers, "x-caption-b64")?;

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
    let response = FILE_HTTP_CLIENT.get(link).send().await?;

    // Intentionally: any non-200 (success or error) collapses to `None`
    // here, unlike the other functions in this module — this doesn't fit
    // check_status's empty-vs-error split and isn't a bug spec 08 called
    // out, so it's left as-is rather than "consistency-fixed".
    if response.status() != StatusCode::OK {
        return Ok(None);
    };

    Ok(Some(DownloadFile {
        response,
        filename: filename.to_string(),
        caption: "".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn absent_books_accept_old_and_new_tfcs_statuses() {
        for status in [204, 404] {
            let response = reqwest::Response::from(
                http::Response::builder()
                    .status(status)
                    .body(Vec::<u8>::new())
                    .unwrap(),
            );
            assert!(
                super::check_response::<serde_json::Value>(response, super::ABSENT_STATUSES)
                    .await
                    .unwrap()
                    .is_none()
            );
            let response = reqwest::Response::from(
                http::Response::builder()
                    .status(status)
                    .body(Vec::<u8>::new())
                    .unwrap(),
            );
            assert!(super::check_status(response, super::ABSENT_STATUSES)
                .await
                .unwrap()
                .is_none());
        }
    }
    use super::decode_b64_header;
    use reqwest::header::HeaderMap;

    #[test]
    fn missing_header_returns_err() {
        let headers = HeaderMap::new();
        assert!(decode_b64_header(&headers, "x-filename-b64").is_err());
    }

    #[test]
    fn invalid_base64_returns_err() {
        let mut headers = HeaderMap::new();
        headers.insert("x-test", "not!!base64".parse().unwrap());
        assert!(decode_b64_header(&headers, "x-test").is_err());
    }

    #[test]
    fn non_utf8_returns_err() {
        use base64::{engine::general_purpose, Engine as _};
        let mut headers = HeaderMap::new();
        let encoded = general_purpose::STANDARD.encode([0xFF, 0xFE]);
        headers.insert("x-test", encoded.parse().unwrap());
        assert!(decode_b64_header(&headers, "x-test").is_err());
    }

    #[test]
    fn valid_header_decoded() {
        use base64::{engine::general_purpose, Engine as _};
        let mut headers = HeaderMap::new();
        let encoded = general_purpose::STANDARD.encode("hello.epub");
        headers.insert("x-test", encoded.parse().unwrap());
        assert_eq!(decode_b64_header(&headers, "x-test").unwrap(), "hello.epub");
    }
}
