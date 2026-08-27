use std::collections::HashSet;

use smartstring::alias::String as SmartString;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::bots::approved_bot::services::user_settings::{DefaultSearchType, FileNameLang, Lang};

use super::callback_data::SettingsCallbackData;

pub fn get_main_settings_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            "Языки",
            SettingsCallbackData::Settings.to_string(),
        )],
        vec![InlineKeyboardButton::callback(
            "Поиск по умолчанию",
            SettingsCallbackData::DefaultSearchMenu.to_string(),
        )],
        vec![InlineKeyboardButton::callback(
            "Имена файлов",
            SettingsCallbackData::FileNameLangMenu.to_string(),
        )],
    ])
}

pub fn get_lang_keyboard(
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

            vec![InlineKeyboardButton::callback(
                format!("{emoji} {}", lang.label),
                callback_data,
            )]
        })
        .collect();

    buttons.push(vec![InlineKeyboardButton::callback(
        "← Назад",
        SettingsCallbackData::LangSettingsBack.to_string(),
    )]);

    InlineKeyboardMarkup::new(buttons)
}

pub fn get_default_search_keyboard(current: Option<DefaultSearchType>) -> InlineKeyboardMarkup {
    let check = |v: DefaultSearchType| if current == Some(v) { " ✓" } else { "" };
    InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            format!("Книга{}", check(DefaultSearchType::Book)),
            SettingsCallbackData::DefaultSearch {
                value: "book".into(),
            }
            .to_string(),
        )],
        vec![InlineKeyboardButton::callback(
            format!("Автор{}", check(DefaultSearchType::Author)),
            SettingsCallbackData::DefaultSearch {
                value: "author".into(),
            }
            .to_string(),
        )],
        vec![InlineKeyboardButton::callback(
            format!("Серия{}", check(DefaultSearchType::Series)),
            SettingsCallbackData::DefaultSearch {
                value: "series".into(),
            }
            .to_string(),
        )],
        vec![InlineKeyboardButton::callback(
            format!("Переводчик{}", check(DefaultSearchType::Translator)),
            SettingsCallbackData::DefaultSearch {
                value: "translator".into(),
            }
            .to_string(),
        )],
        vec![InlineKeyboardButton::callback(
            format!("Не выбрано{}", if current.is_none() { " ✓" } else { "" }),
            SettingsCallbackData::DefaultSearch {
                value: "none".into(),
            }
            .to_string(),
        )],
        vec![InlineKeyboardButton::callback(
            "← Назад",
            SettingsCallbackData::DefaultSearchBack.to_string(),
        )],
    ])
}

pub fn get_file_name_lang_keyboard(current: FileNameLang) -> InlineKeyboardMarkup {
    let check = |v: FileNameLang| if current == v { " ✓" } else { "" };
    InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            format!("Транслит{}", check(FileNameLang::Normalized)),
            SettingsCallbackData::FileNameLang {
                value: FileNameLang::Normalized.as_api_str().into(),
            }
            .to_string(),
        )],
        vec![InlineKeyboardButton::callback(
            format!("Язык оригинала{}", check(FileNameLang::Original)),
            SettingsCallbackData::FileNameLang {
                value: FileNameLang::Original.as_api_str().into(),
            }
            .to_string(),
        )],
        vec![InlineKeyboardButton::callback(
            "← Назад",
            SettingsCallbackData::FileNameLangBack.to_string(),
        )],
    ])
}
