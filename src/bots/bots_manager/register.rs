use std::collections::HashMap;

use teloxide::prelude::*;

use crate::config;


#[derive(Debug)]
pub enum RegisterStatus {
    Success {username: String},
    NoToken,
    WrongToken,
    RegisterFail,
}


async fn get_bot_username(token: &str) -> Option<String> {
    match Bot::new(token).get_me().send().await {
        Ok(v) => v.username.clone(),
        Err(_) => None
    }
}

async fn make_register_request(user_id: UserId, username: &str, token: &str) -> Result<(), ()> {
    let user_id = &user_id.to_string();

    let data = HashMap::from([
        ("token", token),
        ("user", user_id),
        ("username", username),
        ("status", "pending"),
        ("cache", "no_cache")
    ]);

    let client = reqwest::Client::new();
    let response = client
        .post(config::CONFIG.manager_url.clone())
        .header("Authorization", config::CONFIG.manager_api_key.clone())
        .json(&data)
        .send()
        .await;

    let status_code = match response {
        Ok(v) => v.status(),
        Err(_) => return Err(()),
    };

    log::debug!("make_register_request status_code={}", status_code);

    if status_code != 200 {
        return Err(());
    }

    Ok(())
}


pub async fn register(user_id: UserId, message_text: &str) -> RegisterStatus {
    let token = match super::utils::get_token(message_text) {
        Some(v) => v,
        None => return RegisterStatus::NoToken
    };

    let bot_username = match get_bot_username(token).await {
        Some(v) => v,
        None => return RegisterStatus::WrongToken
    };

    let register_request_status = make_register_request(user_id, &bot_username, token).await;

    if register_request_status.is_err() {
        return RegisterStatus::RegisterFail;
    }

    return RegisterStatus::Success { username: bot_username };
}
