pub mod callback_data;
pub mod commands;

use std::collections::HashSet;

use smartstring::alias::String as SmartString;

use crate::bots::{
    approved_bot::{
        services::user_settings::{
            create_or_update_user_settings, get_langs, get_user_or_default_lang_codes, Lang,
        },
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};

use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Me},
};

use self::{callback_data::SettingsCallbackData, commands::SettingsCommand};

async fn settings_handler(message: Message, bot: CacheMe<Throttle<Bot>>) -> BotHandlerInternal {
    let keyboard = InlineKeyboardMarkup {
        inline_keyboard: vec![vec![InlineKeyboardButton {
            text: "Языки".to_string(),
            kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                SettingsCallbackData::Settings.to_string(),
            ),
        }]],
    };

    bot.send_message(message.chat.id, "Настройки")
        .reply_markup(keyboard)
        .send()
        .await?;

    Ok(())
}

fn get_lang_keyboard(
    all_langs: Vec<Lang>,
    allowed_langs: HashSet<SmartString>,
) -> InlineKeyboardMarkup {
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
) -> BotHandlerInternal {
    let message = match cq.message {
        Some(v) => v,
        None => {
            bot.send_message(cq.from.id, "Ошибка! Попробуйте заново(")
                .send()
                .await?;
            return Ok(());
        }
    };

    let user = cq.from;

    let allowed_langs = get_user_or_default_lang_codes(user.id).await;

    let mut allowed_langs_set: HashSet<SmartString> = HashSet::new();
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
        bot.answer_callback_query(cq.id)
            .text("Должен быть активен, хотя бы один язык!")
            .show_alert(true)
            .send()
            .await?;

        return Ok(());
    }

    if let Err(err) = create_or_update_user_settings(
        user.id,
        user.last_name.clone().unwrap_or("".to_string()),
        user.first_name.clone(),
        user.username.clone().unwrap_or("".to_string()),
        me.username.clone().unwrap(),
        allowed_langs_set.clone().into_iter().collect(),
    )
    .await
    {
        bot.send_message(message.chat().id, "Ошибка! Попробуйте заново(")
            .send()
            .await?;
        return Err(err);
    }

    let all_langs = match get_langs().await {
        Ok(v) => v,
        Err(err) => {
            bot.send_message(message.chat().id, "Ошибка! Попробуйте заново(")
                .send()
                .await?;
            return Err(err);
        }
    };

    let keyboard = get_lang_keyboard(all_langs, allowed_langs_set);

    bot.edit_message_reply_markup(message.chat().id, message.id())
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
                    .endpoint(settings_handler),
            ),
        )
        .branch(
            Update::filter_callback_query()
                .chain(filter_callback_query::<SettingsCallbackData>())
                .endpoint(settings_callback_handler),
        )
}
