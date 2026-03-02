pub mod callback_data;
pub mod commands;

use book_bot_macros::log_handler;

use std::collections::HashSet;

use smallvec::SmallVec;
use smartstring::alias::String as SmartString;

use crate::bots::{
    approved_bot::{
        services::user_settings::{
            create_or_update_user_settings, get_langs, get_user_or_default_lang_codes,
            get_user_settings, DefaultSearchType, Lang,
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

fn get_main_settings_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup {
        inline_keyboard: vec![
            vec![InlineKeyboardButton {
                text: "Языки".to_string(),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::Settings.to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "Поиск по умолчанию".to_string(),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearchMenu.to_string(),
                ),
            }],
        ],
    }
}

#[log_handler("settings")]
async fn settings_handler(message: Message, bot: CacheMe<Throttle<Bot>>) -> BotHandlerInternal {
    bot.send_message(message.chat.id, "Настройки")
        .reply_markup(get_main_settings_keyboard())
        .send()
        .await?;

    Ok(())
}

fn get_lang_keyboard(
    all_langs: Vec<Lang>,
    allowed_langs: HashSet<SmartString>,
) -> InlineKeyboardMarkup {
    let mut buttons: Vec<Vec<InlineKeyboardButton>> = all_langs
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

    buttons.push(vec![InlineKeyboardButton {
        text: "← Назад".to_string(),
        kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
            SettingsCallbackData::LangSettingsBack.to_string(),
        ),
    }]);

    InlineKeyboardMarkup {
        inline_keyboard: buttons,
    }
}

fn get_default_search_keyboard(current: Option<DefaultSearchType>) -> InlineKeyboardMarkup {
    let check = |v: DefaultSearchType| if current == Some(v) { " ✓" } else { "" };
    InlineKeyboardMarkup {
        inline_keyboard: vec![
            vec![InlineKeyboardButton {
                text: format!("Книга{}", check(DefaultSearchType::Book)),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearch {
                        value: "book".into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: format!("Автор{}", check(DefaultSearchType::Author)),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearch {
                        value: "author".into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: format!("Серия{}", check(DefaultSearchType::Series)),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearch {
                        value: "series".into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: format!("Переводчик{}", check(DefaultSearchType::Translator)),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearch {
                        value: "translator".into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: format!("Не выбрано{}", if current.is_none() { " ✓" } else { "" }),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearch {
                        value: "none".into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "← Назад".to_string(),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearchBack.to_string(),
                ),
            }],
        ],
    }
}

#[log_handler("settings")]
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

    match &callback_data {
        SettingsCallbackData::DefaultSearchMenu => {
            let current = get_user_settings(user.id).await.ok().flatten();
            let current_default = current.as_ref().and_then(|s| s.default_search);
            let keyboard = get_default_search_keyboard(current_default);
            bot.edit_message_text(message.chat().id, message.id(), "Поиск по умолчанию")
                .reply_markup(keyboard)
                .send()
                .await?;
            bot.answer_callback_query(cq.id).send().await?;
            return Ok(());
        }
        SettingsCallbackData::DefaultSearchBack => {
            bot.edit_message_text(message.chat().id, message.id(), "Настройки")
                .reply_markup(get_main_settings_keyboard())
                .send()
                .await?;
            bot.answer_callback_query(cq.id).send().await?;
            return Ok(());
        }
        SettingsCallbackData::LangSettingsBack => {
            bot.edit_message_text(message.chat().id, message.id(), "Настройки")
                .reply_markup(get_main_settings_keyboard())
                .send()
                .await?;
            bot.answer_callback_query(cq.id).send().await?;
            return Ok(());
        }
        SettingsCallbackData::DefaultSearch { value } => {
            let current = get_user_settings(user.id).await.ok().flatten();
            let allowed_langs: SmallVec<[SmartString; 3]> = match current {
                Some(s) => s.allowed_langs.into_iter().map(|l| l.code).collect(),
                None => get_user_or_default_lang_codes(user.id).await,
            };
            let default_search = if value.as_str() == "none" {
                None
            } else if let Some(t) = DefaultSearchType::from_api_str(value.as_str()) {
                Some(t)
            } else {
                bot.answer_callback_query(cq.id).send().await?;
                return Ok(());
            };
            if create_or_update_user_settings(
                user.id,
                &user.last_name.unwrap_or("".to_string()),
                &user.first_name,
                user.username.as_deref().unwrap_or(""),
                &me.username.clone().unwrap(),
                allowed_langs,
                default_search,
            )
            .await
            .is_err()
            {
                bot.answer_callback_query(cq.id)
                    .text("Ошибка! Попробуйте заново(")
                    .show_alert(true)
                    .send()
                    .await?;
                return Ok(());
            }
            bot.edit_message_text(message.chat().id, message.id(), "Настройки")
                .reply_markup(get_main_settings_keyboard())
                .send()
                .await?;
            bot.answer_callback_query(cq.id)
                .text("Готово")
                .send()
                .await?;
            return Ok(());
        }
        _ => {}
    }

    let allowed_langs = get_user_or_default_lang_codes(user.id).await;

    let mut allowed_langs_set: HashSet<SmartString> = HashSet::new();
    allowed_langs.into_iter().for_each(|v| {
        allowed_langs_set.insert(v);
    });

    match &callback_data {
        SettingsCallbackData::Settings => (),
        SettingsCallbackData::On { code } => {
            allowed_langs_set.insert(code.clone());
        }
        SettingsCallbackData::Off { code } => {
            allowed_langs_set.remove(code);
        }
        SettingsCallbackData::LangSettingsBack
        | SettingsCallbackData::DefaultSearchBack
        | SettingsCallbackData::DefaultSearchMenu
        | SettingsCallbackData::DefaultSearch { .. } => {}
    };

    if allowed_langs_set.is_empty() {
        bot.answer_callback_query(cq.id)
            .text("Должен быть активен, хотя бы один язык!")
            .show_alert(true)
            .send()
            .await?;

        return Ok(());
    }

    let current_settings = get_user_settings(user.id).await.ok().flatten();
    let default_search = current_settings.as_ref().and_then(|s| s.default_search);

    if let Err(err) = create_or_update_user_settings(
        user.id,
        &user.last_name.unwrap_or("".to_string()),
        &user.first_name,
        &user.username.unwrap_or("".to_string()),
        &me.username.clone().unwrap(),
        allowed_langs_set.clone().into_iter().collect(),
        default_search,
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
