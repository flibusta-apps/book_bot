use std::error::Error;

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
}

#[derive(Debug)]
pub enum RegisterRequestStatus {
    Success,
    LimitExtended,
    UnknownError,
}

async fn get_bot_username(token: &str) -> Option<String> {
    match Bot::new(token).get_me().send().await {
        Ok(v) => v.username.clone(),
        Err(err) => {
            log::error!("Bot reg (getting username) error: {err:?}");
            None
        }
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

    let result = reqwest::Client::new()
        .post(config::CONFIG.manager_url.clone())
        .body(body.to_string())
        .header("Authorization", config::CONFIG.manager_api_key.clone())
        .header("Content-Type", "application/json")
        .send()
        .await?;

    Ok(match result.status().as_u16() {
        200 => RegisterRequestStatus::Success,
        402 => RegisterRequestStatus::LimitExtended,
        _ => RegisterRequestStatus::UnknownError,
    })
}

pub async fn register(user_id: UserId, message_text: &str) -> RegisterStatus {
    let token = super::utils::get_token(message_text).unwrap();

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
    }
}
