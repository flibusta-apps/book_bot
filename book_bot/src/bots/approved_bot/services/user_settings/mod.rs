use moka::future::Cache;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use smallvec::{smallvec, SmallVec};
use smartstring::alias::String as SmartString;
use std::sync::LazyLock;
use std::time::Duration;
use teloxide::types::{ChatId, UserId};
use tracing::log;

use crate::{
    bots::approved_bot::services::{build_url, check_response, check_status, HTTP_CLIENT},
    config,
};

/// API values: "book" | "author" | "series" | "translator"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DefaultSearchType {
    Book,
    Author,
    Series,
    Translator,
}

impl DefaultSearchType {
    pub fn as_api_str(self) -> &'static str {
        match self {
            DefaultSearchType::Book => "book",
            DefaultSearchType::Author => "author",
            DefaultSearchType::Series => "series",
            DefaultSearchType::Translator => "translator",
        }
    }

    pub fn from_api_str(s: &str) -> Option<Self> {
        match s {
            "book" => Some(DefaultSearchType::Book),
            "author" => Some(DefaultSearchType::Author),
            "series" => Some(DefaultSearchType::Series),
            "translator" => Some(DefaultSearchType::Translator),
            _ => None,
        }
    }
}

/// API values: "normalized" | "original"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FileNameLang {
    #[default]
    Normalized,
    Original,
}

impl FileNameLang {
    pub fn as_api_str(self) -> &'static str {
        match self {
            FileNameLang::Normalized => "normalized",
            FileNameLang::Original => "original",
        }
    }

    pub fn from_api_str(s: &str) -> Option<Self> {
        match s {
            "normalized" => Some(FileNameLang::Normalized),
            "original" => Some(FileNameLang::Original),
            _ => None,
        }
    }
}

fn deserialize_optional_default_search<'de, D>(d: D) -> Result<Option<DefaultSearchType>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(d)?;
    Ok(opt.and_then(|s| DefaultSearchType::from_api_str(&s)))
}

#[derive(Deserialize, Debug, Clone)]
pub struct Lang {
    // pub id: u32,
    pub label: SmartString,
    pub code: SmartString,
}

#[derive(Deserialize, Debug, Clone)]
pub struct UserSettings {
    // pub user_id: u64,
    // pub last_name: SmartString,
    // pub first_name: SmartString,
    // pub username: SmartString,
    // pub source: SmartString,
    pub allowed_langs: SmallVec<[Lang; 3]>,
    #[serde(default, deserialize_with = "deserialize_optional_default_search")]
    pub default_search: Option<DefaultSearchType>,
    #[serde(default)]
    pub file_name_lang: FileNameLang,
}

pub static USER_SETTINGS_CACHE: LazyLock<Cache<UserId, Option<UserSettings>>> =
    LazyLock::new(|| {
        Cache::builder()
            .time_to_live(Duration::from_secs(30 * 60))
            .max_capacity(4096)
            .build()
    });

/// Loads the user's settings through `USER_SETTINGS_CACHE`. Concurrent
/// misses for the same user are coalesced into one HTTP request via
/// `try_get_with`. `Ok(None)` (the user has no settings yet) is a valid,
/// cacheable value; request errors are logged and never cached, so a
/// struggling user-settings service does not "stick" a stale default past
/// its own recovery.
async fn get_cached_user_settings(user_id: UserId) -> Option<UserSettings> {
    match USER_SETTINGS_CACHE
        .try_get_with(user_id, get_user_settings(user_id))
        .await
    {
        Ok(settings) => settings,
        Err(err) => {
            log::error!("{err:?}");
            None
        }
    }
}

pub async fn get_user_settings(user_id: UserId) -> anyhow::Result<Option<UserSettings>> {
    let url = build_url(
        &config::CONFIG.user_settings_url,
        ["users", &user_id.to_string()],
    )?;

    let response = HTTP_CLIENT
        .get(url)
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?;

    check_response(response, &[StatusCode::NOT_FOUND, StatusCode::NO_CONTENT]).await
}

pub async fn get_user_or_default_lang_codes(user_id: UserId) -> SmallVec<[SmartString; 3]> {
    let default_lang_codes = smallvec!["ru".into(), "be".into(), "uk".into()];

    match get_cached_user_settings(user_id).await {
        Some(settings) => settings
            .allowed_langs
            .into_iter()
            .map(|lang| lang.code)
            .collect(),
        None => default_lang_codes,
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn create_or_update_user_settings(
    user_id: UserId,
    last_name: &str,
    first_name: &str,
    username: &str,
    source: &str,
    allowed_langs: SmallVec<[SmartString; 3]>,
    default_search: Option<DefaultSearchType>,
    file_name_lang: FileNameLang,
) -> anyhow::Result<UserSettings> {
    USER_SETTINGS_CACHE.invalidate(&user_id).await;

    let default_search_json = match &default_search {
        Some(t) => serde_json::Value::String(t.as_api_str().to_string()),
        None => serde_json::Value::Null,
    };
    let body = json!({
        "user_id": user_id,
        "last_name": last_name,
        "first_name": first_name,
        "username": username,
        "source": source,
        "allowed_langs": allowed_langs.into_vec(),
        "default_search": default_search_json,
        "file_name_lang": file_name_lang.as_api_str(),
    });

    let url = build_url(&config::CONFIG.user_settings_url, ["users", ""])?;

    let response = HTTP_CLIENT
        .post(url)
        .body(body.to_string())
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .header("Content-Type", "application/json")
        .send()
        .await?;

    check_response(response, &[])
        .await?
        .ok_or_else(|| anyhow::anyhow!("user-settings service returned an empty response"))
}

/// Thin wrapper around `create_or_update_user_settings` that does the
/// `Option<String>` → `&str` unpacking shared by every settings-mutation
/// call site (`settings::mod`'s three handlers, and the background
/// activity-update fallback in `approved_bot::mod`).
pub async fn save_user_settings(
    user: &teloxide::types::User,
    me: &teloxide::types::Me,
    allowed_langs: SmallVec<[SmartString; 3]>,
    default_search: Option<DefaultSearchType>,
    file_name_lang: FileNameLang,
) -> anyhow::Result<UserSettings> {
    create_or_update_user_settings(
        user.id,
        user.last_name.as_deref().unwrap_or(""),
        &user.first_name,
        user.username.as_deref().unwrap_or(""),
        me.username.as_deref().unwrap_or_default(),
        allowed_langs,
        default_search,
        file_name_lang,
    )
    .await
}

/// Returns the user's default search type from the shared settings cache.
/// `None` if not set, the user has no settings, or the request failed.
pub async fn get_user_default_search(user_id: UserId) -> Option<DefaultSearchType> {
    get_cached_user_settings(user_id)
        .await
        .and_then(|settings| settings.default_search)
}

/// Returns the user's `file_name_lang` setting via the shared settings
/// cache. On any error or missing user, returns the default (`Normalized`).
pub async fn get_user_file_name_lang(user_id: UserId) -> FileNameLang {
    get_cached_user_settings(user_id)
        .await
        .map(|settings| settings.file_name_lang)
        .unwrap_or_default()
}

/// Resolve `file_name_lang` for an `Option<u64>`. `None` means there is
/// no user context (e.g. an internal call) and we fall back to the
/// default, which is `Normalized`.
pub async fn get_user_file_name_lang_for(user_id: Option<u64>) -> FileNameLang {
    match user_id {
        Some(uid) => get_user_file_name_lang(UserId(uid)).await,
        None => FileNameLang::default(),
    }
}

pub async fn get_langs() -> anyhow::Result<Vec<Lang>> {
    let url = build_url(&config::CONFIG.user_settings_url, ["languages", ""])?;

    let response = HTTP_CLIENT
        .get(url)
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?;

    check_response(response, &[])
        .await?
        .ok_or_else(|| anyhow::anyhow!("user-settings service returned an empty response"))
}

pub async fn update_user_activity(user_id: UserId) -> anyhow::Result<()> {
    let url = build_url(
        &config::CONFIG.user_settings_url,
        ["users", &user_id.to_string(), "update_activity"],
    )?;

    let response = HTTP_CLIENT
        .post(url)
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?;

    check_status(response, &[]).await?;

    Ok(())
}

pub async fn is_need_donate_notifications(
    chat_id: ChatId,
    is_private: bool,
) -> anyhow::Result<bool> {
    let url = build_url(
        &config::CONFIG.user_settings_url,
        ["donate_notifications", &chat_id.to_string(), "is_need_send"],
    )?;

    let response = HTTP_CLIENT
        .get(url)
        .query(&[("is_private", is_private.to_string())])
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?;

    check_response(response, &[])
        .await?
        .ok_or_else(|| anyhow::anyhow!("user-settings service returned an empty response"))
}

pub async fn mark_donate_notification_sent(chat_id: ChatId) -> anyhow::Result<()> {
    let url = build_url(
        &config::CONFIG.user_settings_url,
        ["donate_notifications", &chat_id.to_string()],
    )?;

    let response = HTTP_CLIENT
        .post(url)
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?;

    check_status(response, &[]).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn try_get_with_never_caches_an_error() {
        let cache: Cache<u32, Option<u32>> = Cache::builder().build();
        let key = 1u32;

        let err_result = cache
            .try_get_with(key, async {
                Err::<Option<u32>, anyhow::Error>(anyhow::anyhow!("boom"))
            })
            .await;
        assert!(err_result.is_err());
        assert!(
            !cache.contains_key(&key),
            "an error must not be inserted into the cache"
        );

        let ok_result = cache
            .try_get_with(key, async { Ok::<_, anyhow::Error>(Some(42u32)) })
            .await
            .unwrap();
        assert_eq!(ok_result, Some(42));
        assert!(cache.contains_key(&key));
    }

    #[tokio::test]
    async fn a_404_from_the_user_settings_service_is_not_an_error() {
        use crate::bots::approved_bot::services::check_response;

        let http_response = http::Response::builder()
            .status(404)
            .body(Vec::<u8>::new())
            .unwrap();
        let response = reqwest::Response::from(http_response);

        let result: anyhow::Result<Option<UserSettings>> =
            check_response(response, &[StatusCode::NOT_FOUND, StatusCode::NO_CONTENT]).await;

        assert!(result.is_ok(), "404 must not be an Err");
        assert!(
            result.unwrap().is_none(),
            "404 must mean 'no settings for this user', i.e. None"
        );
    }
}
