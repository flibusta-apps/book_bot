use std::{collections::HashSet, str::FromStr, vec};

use crate::{bots::{
    approved_bot::{
        services::user_settings::{
            create_or_update_user_settings, get_langs, get_user_or_default_lang_codes, Lang,
        },
        tools::filter_callback_query,
    },
    BotHandlerInternal,
}, bots_manager::AppState};

use moka::future::Cache;
use regex::Regex;
use smallvec::SmallVec;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Me},
    utils::command::BotCommands, adaptors::{Throttle, CacheMe},
};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum SettingsCommand {
    Settings,
}

#[derive(Clone)]
enum SettingsCallbackData {
    Settings,
    On { code: String },
    Off { code: String },
}

impl FromStr for SettingsCallbackData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == SettingsCallbackData::Settings.to_string().as_str() {
            return Ok(SettingsCallbackData::Settings);
        }

        let re = Regex::new(r"^lang_(?P<action>(off)|(on))_(?P<code>[a-zA-z]+)$").unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let action = &caps["action"];
        let code = caps["code"].to_string();

        match action {
            "on" => Ok(SettingsCallbackData::On { code }),
            "off" => Ok(SettingsCallbackData::Off { code }),
            _ => Err(strum::ParseError::VariantNotFound),
        }
    }
}

impl ToString for SettingsCallbackData {
    fn to_string(&self) -> String {
        match self {
            SettingsCallbackData::Settings => "lang_settings".to_string(),
            SettingsCallbackData::On { code } => format!("lang_on_{code}"),
            SettingsCallbackData::Off { code } => format!("lang_off_{code}"),
        }
    }
}

async fn settings_handler(message: Message, bot: CacheMe<Throttle<Bot>>) -> BotHandlerInternal {
    let keyboard = InlineKeyboardMarkup {
        inline_keyboard: vec![vec![InlineKeyboardButton {
            text: "Языки".to_string(),
            kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                SettingsCallbackData::Settings.to_string(),
            ),
        }]],
    };

    bot
        .send_message(message.chat.id, "Настройки")
        .reply_markup(keyboard)
        .send()
        .await?;

    Ok(())
}

fn get_lang_keyboard(all_langs: Vec<Lang>, allowed_langs: HashSet<String>) -> InlineKeyboardMarkup {
    let buttons = all_langs
        .into_iter()
        .map(|lang| {
            let (emoji, callback_data) = match allowed_langs.contains(&lang.code) {
                true => (
                    "🟢".to_string(),
                    SettingsCallbackData::Off { code: lang.code }.to_string(),
                ),
                false => (
                    "🔴".to_string(),
                    SettingsCallbackData::On { code: lang.code }.to_string(),
                ),
            };

            vec![InlineKeyboardButton {
                text: format!("{emoji} {}", lang.label),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(callback_data),
            }]
        })
        .collect();

    InlineKeyboardMarkup {
        inline_keyboard: buttons,
    }
}

async fn settings_callback_handler(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    callback_data: SettingsCallbackData,
    me: Me,
    user_langs_cache: Cache<UserId, SmallVec<[String; 3]>>,
) -> BotHandlerInternal {
    let message = match cq.message {
        Some(v) => v,
        None => {
            bot.send_message(cq.from.id, "Ошибка! Попробуйте заново(").send().await?;
            return Ok(());
        },
    };

    let user = cq.from;

    let allowed_langs = get_user_or_default_lang_codes(user.id, user_langs_cache.clone()).await;

    let mut allowed_langs_set: HashSet<String> = HashSet::new();
    allowed_langs.into_iter().for_each(|v| {
        allowed_langs_set.insert(v);
    });

    match callback_data {
        SettingsCallbackData::Settings => (),
        SettingsCallbackData::On { code } => {
            allowed_langs_set.insert(code);
        }
        SettingsCallbackData::Off { code } => {
            allowed_langs_set.remove(&code);
        }
    };

    if allowed_langs_set.is_empty() {
        bot
            .answer_callback_query(cq.id)
            .text("Должен быть активен, хотя бы один язык!")
            .show_alert(true)
            .send()
            .await?;

        return Ok(())
    }

    if let Err(err) = create_or_update_user_settings(
        user.id,
        user.last_name.clone().unwrap_or("".to_string()),
        user.first_name.clone(),
        user.username.clone().unwrap_or("".to_string()),
        me.username.clone().unwrap(),
        allowed_langs_set.clone().into_iter().collect(),
        user_langs_cache,
    )
    .await {
        bot.send_message(user.id, "Ошибка! Попробуйте заново(").send().await?;
        return Err(err);
    }

    let all_langs = match get_langs().await {
        Ok(v) => v,
        Err(err) => {
            bot.send_message(user.id, "Ошибка! Попробуйте заново(").send().await?;
            return Err(err)
        },
    };

    let keyboard = get_lang_keyboard(all_langs, allowed_langs_set);

    bot
        .edit_message_reply_markup(message.chat.id, message.id)
        .reply_markup(keyboard)
        .send()
        .await?;

    Ok(())
}

pub fn get_settings_handler() -> crate::bots::BotHandler {
    dptree::entry()
        .branch(
            Update::filter_message().branch(
                dptree::entry()
                    .filter_command::<SettingsCommand>()
                    .endpoint(|message, bot| async move { settings_handler(message, bot).await }),
            ),
        )
        .branch(
            Update::filter_callback_query()
                .chain(filter_callback_query::<SettingsCallbackData>())
                .endpoint(
                    |cq: CallbackQuery,
                     bot: CacheMe<Throttle<Bot>>,
                     callback_data: SettingsCallbackData,
                     me: Me,
                     app_state: AppState| async move {
                        settings_callback_handler(cq, bot, callback_data, me, app_state.user_langs_cache).await
                    },
                ),
        )
}
