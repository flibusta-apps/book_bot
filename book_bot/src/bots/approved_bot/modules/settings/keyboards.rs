use std::collections::HashSet;

use smartstring::alias::String as SmartString;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardButtonKind, InlineKeyboardMarkup};

use crate::bots::approved_bot::services::user_settings::{DefaultSearchType, FileNameLang, Lang};

use super::callback_data::SettingsCallbackData;

pub fn get_main_settings_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup {
        inline_keyboard: vec![
            vec![InlineKeyboardButton {
                text: "Языки".to_string(),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::Settings.to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "Поиск по умолчанию".to_string(),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearchMenu.to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "Имена файлов".to_string(),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::FileNameLangMenu.to_string(),
                ),
            }],
        ],
    }
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

            vec![InlineKeyboardButton {
                text: format!("{emoji} {}", lang.label),
                kind: InlineKeyboardButtonKind::CallbackData(callback_data),
            }]
        })
        .collect();

    buttons.push(vec![InlineKeyboardButton {
        text: "← Назад".to_string(),
        kind: InlineKeyboardButtonKind::CallbackData(
            SettingsCallbackData::LangSettingsBack.to_string(),
        ),
    }]);

    InlineKeyboardMarkup {
        inline_keyboard: buttons,
    }
}

pub fn get_default_search_keyboard(current: Option<DefaultSearchType>) -> InlineKeyboardMarkup {
    let check = |v: DefaultSearchType| if current == Some(v) { " ✓" } else { "" };
    InlineKeyboardMarkup {
        inline_keyboard: vec![
            vec![InlineKeyboardButton {
                text: format!("Книга{}", check(DefaultSearchType::Book)),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearch {
                        value: "book".into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: format!("Автор{}", check(DefaultSearchType::Author)),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearch {
                        value: "author".into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: format!("Серия{}", check(DefaultSearchType::Series)),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearch {
                        value: "series".into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: format!("Переводчик{}", check(DefaultSearchType::Translator)),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearch {
                        value: "translator".into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: format!("Не выбрано{}", if current.is_none() { " ✓" } else { "" }),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearch {
                        value: "none".into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "← Назад".to_string(),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearchBack.to_string(),
                ),
            }],
        ],
    }
}

pub fn get_file_name_lang_keyboard(current: FileNameLang) -> InlineKeyboardMarkup {
    let check = |v: FileNameLang| if current == v { " ✓" } else { "" };
    InlineKeyboardMarkup {
        inline_keyboard: vec![
            vec![InlineKeyboardButton {
                text: format!("Транслит{}", check(FileNameLang::Normalized)),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::FileNameLang {
                        value: FileNameLang::Normalized.as_api_str().into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: format!("Язык оригинала{}", check(FileNameLang::Original)),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::FileNameLang {
                        value: FileNameLang::Original.as_api_str().into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "← Назад".to_string(),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::FileNameLangBack.to_string(),
                ),
            }],
        ],
    }
}
