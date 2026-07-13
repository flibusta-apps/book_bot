pub mod callback_data;
pub mod commands;
pub mod keyboards;

use book_bot_macros::log_handler;

use std::collections::HashSet;

use smallvec::SmallVec;
use smartstring::alias::String as SmartString;

use crate::bots::{
    approved_bot::{
        modules::utils::telegram_utils::{
            safe_answer_callback_query, safe_answer_callback_query_with_text,
            safe_edit_message_reply_markup, safe_edit_message_text, safe_send_message,
        },
        services::user_settings::{
            get_langs, get_user_or_default_lang_codes, get_user_settings, save_user_settings,
            DefaultSearchType, FileNameLang,
        },
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};

use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{CallbackQueryId, Me, MessageId},
};

use self::{
    callback_data::SettingsCallbackData,
    commands::SettingsCommand,
    keyboards::{
        get_default_search_keyboard, get_file_name_lang_keyboard, get_lang_keyboard,
        get_main_settings_keyboard,
    },
};

#[log_handler("settings")]
async fn settings_handler(message: Message, bot: CacheMe<Throttle<Bot>>) -> BotHandlerInternal {
    safe_send_message(
        &bot,
        message.chat.id,
        "Настройки",
        Some(get_main_settings_keyboard()),
    )
    .await?;

    Ok(())
}

async fn show_main_menu(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    cq_id: CallbackQueryId,
) -> BotHandlerInternal {
    safe_edit_message_text(
        bot,
        chat_id,
        message_id,
        "Настройки",
        Some(get_main_settings_keyboard()),
    )
    .await?;
    safe_answer_callback_query(bot, cq_id).await?;
    Ok(())
}

async fn show_default_search_menu(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    cq_id: CallbackQueryId,
    user_id: UserId,
) -> BotHandlerInternal {
    let current = get_user_settings(user_id).await.ok().flatten();
    let current_default = current.as_ref().and_then(|s| s.default_search);
    let keyboard = get_default_search_keyboard(current_default);
    safe_edit_message_text(
        bot,
        chat_id,
        message_id,
        "Поиск по умолчанию",
        Some(keyboard),
    )
    .await?;
    safe_answer_callback_query(bot, cq_id).await?;
    Ok(())
}

async fn show_file_name_lang_menu(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    cq_id: CallbackQueryId,
    user_id: UserId,
) -> BotHandlerInternal {
    let current = get_user_settings(user_id).await.ok().flatten();
    let current_value = current
        .as_ref()
        .map(|s| s.file_name_lang)
        .unwrap_or_default();
    let keyboard = get_file_name_lang_keyboard(current_value);
    safe_edit_message_text(bot, chat_id, message_id, "Имена файлов", Some(keyboard)).await?;
    safe_answer_callback_query(bot, cq_id).await?;
    Ok(())
}

async fn handle_default_search(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    cq_id: CallbackQueryId,
    user: &teloxide::types::User,
    me: &Me,
    value: &str,
) -> BotHandlerInternal {
    let current = get_user_settings(user.id).await.ok().flatten();
    let allowed_langs: SmallVec<[SmartString; 3]> = match current.as_ref() {
        Some(s) => s.allowed_langs.iter().map(|l| l.code.clone()).collect(),
        None => get_user_or_default_lang_codes(user.id).await,
    };
    let default_search = if value == "none" {
        None
    } else if let Some(t) = DefaultSearchType::from_api_str(value) {
        Some(t)
    } else {
        safe_answer_callback_query(bot, cq_id).await?;
        return Ok(());
    };
    let file_name_lang = current
        .as_ref()
        .map(|s| s.file_name_lang)
        .unwrap_or_default();

    if save_user_settings(user, me, allowed_langs, default_search, file_name_lang)
        .await
        .is_err()
    {
        safe_answer_callback_query_with_text(bot, cq_id, "Ошибка! Попробуйте заново(", true)
            .await?;
        return Ok(());
    }

    safe_edit_message_text(
        bot,
        chat_id,
        message_id,
        "Настройки",
        Some(get_main_settings_keyboard()),
    )
    .await?;
    safe_answer_callback_query_with_text(bot, cq_id, "Готово", false).await?;
    Ok(())
}

async fn handle_file_name_lang(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    cq_id: CallbackQueryId,
    user: &teloxide::types::User,
    me: &Me,
    value: &str,
) -> BotHandlerInternal {
    let file_name_lang = match FileNameLang::from_api_str(value) {
        Some(v) => v,
        None => {
            safe_answer_callback_query(bot, cq_id).await?;
            return Ok(());
        }
    };
    let current = get_user_settings(user.id).await.ok().flatten();
    let allowed_langs: SmallVec<[SmartString; 3]> = match current.as_ref() {
        Some(s) => s.allowed_langs.iter().map(|l| l.code.clone()).collect(),
        None => get_user_or_default_lang_codes(user.id).await,
    };
    let default_search = current.as_ref().and_then(|s| s.default_search);

    if save_user_settings(user, me, allowed_langs, default_search, file_name_lang)
        .await
        .is_err()
    {
        safe_answer_callback_query_with_text(bot, cq_id, "Ошибка! Попробуйте заново(", true)
            .await?;
        return Ok(());
    }

    safe_edit_message_text(
        bot,
        chat_id,
        message_id,
        "Настройки",
        Some(get_main_settings_keyboard()),
    )
    .await?;
    safe_answer_callback_query_with_text(bot, cq_id, "Готово", false).await?;
    Ok(())
}

async fn handle_lang_toggle(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    cq_id: CallbackQueryId,
    user: &teloxide::types::User,
    me: &Me,
    callback_data: &SettingsCallbackData,
) -> BotHandlerInternal {
    let allowed_langs = get_user_or_default_lang_codes(user.id).await;

    let mut allowed_langs_set: HashSet<SmartString> = HashSet::new();
    allowed_langs.into_iter().for_each(|v| {
        allowed_langs_set.insert(v);
    });

    match callback_data {
        SettingsCallbackData::Settings => (),
        SettingsCallbackData::On { code } => {
            allowed_langs_set.insert(code.clone());
        }
        SettingsCallbackData::Off { code } => {
            allowed_langs_set.remove(code);
        }
        _ => unreachable!("handle_lang_toggle is only called for Settings/On/Off"),
    };

    if allowed_langs_set.is_empty() {
        safe_answer_callback_query_with_text(
            bot,
            cq_id,
            "Должен быть активен, хотя бы один язык!",
            true,
        )
        .await?;

        return Ok(());
    }

    let current_settings = get_user_settings(user.id).await.ok().flatten();
    let default_search = current_settings.as_ref().and_then(|s| s.default_search);
    let file_name_lang = current_settings
        .as_ref()
        .map(|s| s.file_name_lang)
        .unwrap_or_default();

    if let Err(err) = save_user_settings(
        user,
        me,
        allowed_langs_set.clone().into_iter().collect(),
        default_search,
        file_name_lang,
    )
    .await
    {
        safe_send_message(bot, chat_id, "Ошибка! Попробуйте заново(", None).await?;
        return Err(err);
    }

    let all_langs = match get_langs().await {
        Ok(v) => v,
        Err(err) => {
            safe_send_message(bot, chat_id, "Ошибка! Попробуйте заново(", None).await?;
            return Err(err);
        }
    };

    let keyboard = get_lang_keyboard(all_langs, allowed_langs_set);

    safe_edit_message_reply_markup(bot, chat_id, message_id, keyboard).await?;

    Ok(())
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
            safe_send_message(&bot, cq.from.id.into(), "Ошибка! Попробуйте заново(", None).await?;
            return Ok(());
        }
    };

    let user = cq.from;
    let chat_id = message.chat().id;
    let message_id = message.id();

    match &callback_data {
        SettingsCallbackData::DefaultSearchMenu => {
            show_default_search_menu(&bot, chat_id, message_id, cq.id, user.id).await
        }
        SettingsCallbackData::DefaultSearchBack
        | SettingsCallbackData::FileNameLangBack
        | SettingsCallbackData::LangSettingsBack => {
            show_main_menu(&bot, chat_id, message_id, cq.id).await
        }
        SettingsCallbackData::FileNameLangMenu => {
            show_file_name_lang_menu(&bot, chat_id, message_id, cq.id, user.id).await
        }
        SettingsCallbackData::DefaultSearch { value } => {
            handle_default_search(&bot, chat_id, message_id, cq.id, &user, &me, value).await
        }
        SettingsCallbackData::FileNameLang { value } => {
            handle_file_name_lang(&bot, chat_id, message_id, cq.id, &user, &me, value).await
        }
        SettingsCallbackData::Settings
        | SettingsCallbackData::On { .. }
        | SettingsCallbackData::Off { .. } => {
            handle_lang_toggle(&bot, chat_id, message_id, cq.id, &user, &me, &callback_data).await
        }
    }
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
