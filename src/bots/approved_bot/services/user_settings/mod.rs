use serde::Deserialize;
use serde_json::json;
use teloxide::types::UserId;

use crate::config;

#[derive(Deserialize, Debug, Clone)]
pub struct Lang {
    // pub id: u32,
    pub label: String,
    pub code: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct UserSettings {
    pub user_id: u64,
    pub last_name: String,
    pub first_name: String,
    pub username: String,
    pub source: String,
    pub allowed_langs: Vec<Lang>,
}

pub async fn get_user_settings(
    user_id: UserId,
) -> Result<UserSettings, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "{}/users/{}",
            &config::CONFIG.user_settings_url,
            user_id
        ))
        .header("Authorization", &config::CONFIG.user_settings_api_key)
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

    match response.json::<UserSettings>().await {
        Ok(v) => Ok(v),
        Err(err) => Err(Box::new(err)),
    }
}

pub async fn get_user_or_default_lang_codes(user_id: UserId) -> Vec<String> {
    let default_lang_codes = vec![String::from("ru"), String::from("be"), String::from("uk")];

    match get_user_settings(user_id).await {
        Ok(v) => v.allowed_langs.into_iter().map(|lang| lang.code).collect(),
        Err(_) => default_lang_codes,
    }
}

pub async fn create_or_update_user_settings(
    user_id: UserId,
    last_name: String,
    first_name: String,
    username: String,
    source: String,
    allowed_langs: Vec<String>,
) -> Result<UserSettings, Box<dyn std::error::Error + Send + Sync>> {
    let body = json!({
        "user_id": user_id,
        "last_name": last_name,
        "first_name": first_name,
        "username": username,
        "source": source,
        "allowed_langs": allowed_langs
    });

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/users/", &config::CONFIG.user_settings_url))
        .body(body.to_string())
        .header("Authorization", &config::CONFIG.user_settings_api_key)
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

    match response.json::<UserSettings>().await {
        Ok(v) => Ok(v),
        Err(err) => Err(Box::new(err)),
    }
}

pub async fn get_langs() -> Result<Vec<Lang>, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/languages/", &config::CONFIG.user_settings_url))
        .header("Authorization", &config::CONFIG.user_settings_api_key)
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

    match response.json::<Vec<Lang>>().await {
        Ok(v) => Ok(v),
        Err(err) => Err(Box::new(err)),
    }
}
