use once_cell::sync::Lazy;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use smallvec::{smallvec, SmallVec};
use smartstring::alias::String as SmartString;
use teloxide::types::{ChatId, UserId};
use tracing::log;

use crate::{bots_manager::USER_LANGS_CACHE, config};

pub static CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

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
}

pub async fn get_user_settings(
    user_id: UserId,
) -> Result<Option<UserSettings>, Box<dyn std::error::Error + Send + Sync>> {
    let response = CLIENT
        .get(format!(
            "{}/users/{}",
            &config::CONFIG.user_settings_url,
            user_id
        ))
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?
        .error_for_status()?;

    if response.status() == StatusCode::NO_CONTENT {
        return Ok(None);
    }

    Ok(Some(response.json::<UserSettings>().await?))
}

pub async fn get_user_or_default_lang_codes(user_id: UserId) -> SmallVec<[SmartString; 3]> {
    if let Some(cached_langs) = USER_LANGS_CACHE.get(&user_id).await {
        return cached_langs;
    }

    let default_lang_codes = smallvec!["ru".into(), "be".into(), "uk".into()];

    match get_user_settings(user_id).await {
        Ok(v) => {
            let langs: SmallVec<[SmartString; 3]> = match v {
                Some(v) => v.allowed_langs.into_iter().map(|lang| lang.code).collect(),
                None => return default_lang_codes,
            };
            USER_LANGS_CACHE.insert(user_id, langs.clone()).await;
            langs
        }
        Err(err) => {
            log::error!("{err:?}");
            default_lang_codes
        }
    }
}

pub async fn create_or_update_user_settings(
    user_id: UserId,
    last_name: &str,
    first_name: &str,
    username: &str,
    source: &str,
    allowed_langs: SmallVec<[SmartString; 3]>,
    default_search: Option<DefaultSearchType>,
) -> anyhow::Result<UserSettings> {
    USER_LANGS_CACHE.invalidate(&user_id).await;

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
        "default_search": default_search_json
    });

    let response = CLIENT
        .post(format!("{}/users/", &config::CONFIG.user_settings_url))
        .body(body.to_string())
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .header("Content-Type", "application/json")
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<UserSettings>().await?)
}

/// Returns user's default search type from API. None if not set or on error.
pub async fn get_user_default_search(user_id: UserId) -> Option<DefaultSearchType> {
    match get_user_settings(user_id).await {
        Ok(Some(s)) => s.default_search,
        _ => None,
    }
}

pub async fn get_langs() -> anyhow::Result<Vec<Lang>> {
    let response = CLIENT
        .get(format!("{}/languages/", &config::CONFIG.user_settings_url))
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<Vec<Lang>>().await?)
}

pub async fn update_user_activity(user_id: UserId) -> anyhow::Result<()> {
    CLIENT
        .post(format!(
            "{}/users/{user_id}/update_activity",
            &config::CONFIG.user_settings_url
        ))
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}

pub async fn is_need_donate_notifications(
    chat_id: ChatId,
    is_private: bool,
) -> anyhow::Result<bool> {
    let response = CLIENT
        .get(format!(
            "{}/donate_notifications/{chat_id}/is_need_send?is_private={is_private}",
            &config::CONFIG.user_settings_url
        ))
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<bool>().await?)
}

pub async fn mark_donate_notification_sent(chat_id: ChatId) -> anyhow::Result<()> {
    CLIENT
        .post(format!(
            "{}/donate_notifications/{chat_id}",
            &config::CONFIG.user_settings_url
        ))
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}
