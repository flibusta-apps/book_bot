use std::error::Error;
use std::sync::LazyLock;
use std::time::Duration;

use serde_json::json;
use teloxide::prelude::*;
use tracing::log;

use crate::config;

#[derive(Debug)]
pub enum RegisterStatus {
    Success { username: String },
    WrongToken,
    RegisterFail,
    LimitExtended,
    AlreadyExists,
}

#[derive(Debug)]
pub enum RegisterRequestStatus {
    Success,
    LimitExtended,
    AlreadyExists,
    UnknownError,
}

static REGISTER_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .expect("Failed to create registration HTTP client")
});

async fn get_bot_username(token: &str) -> Option<String> {
    let bot = Bot::with_client(token, REGISTER_CLIENT.clone())
        .set_api_url(config::CONFIG.telegram_bot_api.clone());
    match bot.get_me().send().await {
        Ok(v) => v.username.clone(),
        Err(_) => None,
    }
}

async fn make_register_request(
    user_id: UserId,
    username: &str,
    token: &str,
) -> Result<RegisterRequestStatus, Box<dyn Error + Send + Sync>> {
    let body = json!({
        "token": token,
        "user": user_id,
        "status": "approved",
        "cache": "no_cache",
        "username": username,
    });

    let result = REGISTER_CLIENT
        .post(&config::CONFIG.manager_url)
        .bearer_auth(&config::CONFIG.manager_api_key)
        .json(&body)
        .send()
        .await?;

    Ok(match result.status().as_u16() {
        200 => RegisterRequestStatus::Success,
        402 => RegisterRequestStatus::LimitExtended,
        409 => RegisterRequestStatus::AlreadyExists,
        _ => RegisterRequestStatus::UnknownError,
    })
}

pub async fn register(user_id: UserId, message_text: &str) -> RegisterStatus {
    let token = match super::utils::get_token(message_text) {
        Some(t) => t,
        None => return RegisterStatus::WrongToken,
    };

    let bot_username = match get_bot_username(token).await {
        Some(v) => v,
        None => return RegisterStatus::WrongToken,
    };

    let register_request_status = make_register_request(user_id, &bot_username, token).await;

    let result = match register_request_status {
        Ok(v) => v,
        Err(err) => {
            log::error!("Bot reg error: {err:?}");

            return RegisterStatus::RegisterFail;
        }
    };

    match result {
        RegisterRequestStatus::Success => RegisterStatus::Success {
            username: bot_username,
        },
        RegisterRequestStatus::LimitExtended => RegisterStatus::LimitExtended,
        RegisterRequestStatus::UnknownError => RegisterStatus::RegisterFail,
        RegisterRequestStatus::AlreadyExists => RegisterStatus::AlreadyExists,
    }
}
