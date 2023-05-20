use std::error::Error;

use std::collections::HashMap;

use teloxide::prelude::*;

use crate::config;


#[derive(Debug)]
pub enum RegisterStatus {
    Success {username: String},
    WrongToken,
    RegisterFail,
}


async fn get_bot_username(token: &str) -> Option<String> {
    match Bot::new(token).get_me().send().await {
        Ok(v) => v.username.clone(),
        Err(_) => None
    }
}

async fn make_register_request(user_id: UserId, username: &str, token: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let user_id = &user_id.to_string();

    let data = HashMap::from([
        ("token", token),
        ("user", user_id),
        ("username", username),
        ("status", "approved"),
        ("cache", "no_cache")
    ]);

    reqwest::Client::new()
        .post(config::CONFIG.manager_url.clone())
        .header("Authorization", config::CONFIG.manager_api_key.clone())
        .json(&data)
        .send()
        .await?;

    Ok(())
}


pub async fn register(user_id: UserId, message_text: &str) -> RegisterStatus {
    let token = super::utils::get_token(message_text).unwrap();

    let bot_username = match get_bot_username(token).await {
        Some(v) => v,
        None => return RegisterStatus::WrongToken
    };

    let register_request_status = make_register_request(user_id, &bot_username, token).await;

    if let Err(err) = register_request_status {
        log::error!("Bot reg error: {:?}", err);

        return RegisterStatus::RegisterFail;
    }

    RegisterStatus::Success { username: bot_username }
}
