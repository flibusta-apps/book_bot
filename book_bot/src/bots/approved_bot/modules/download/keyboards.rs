use teloxide::types::{InlineKeyboardButton, InlineKeyboardButtonKind, InlineKeyboardMarkup};

use crate::bots::approved_bot::services::book_library::types::Book;

use super::{
    callback_data::{CheckArchiveStatus, DownloadQueryData},
    commands::DownloadArchiveCommand,
};

pub fn get_check_keyboard(task_id: String) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup {
        inline_keyboard: vec![vec![InlineKeyboardButton {
            kind: InlineKeyboardButtonKind::CallbackData(
                (CheckArchiveStatus { task_id }).to_string(),
            ),
            text: String::from("Обновить статус"),
        }]],
    }
}

pub fn get_download_format_keyboard(book: &Book) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup {
        inline_keyboard: book
            .available_types
            .iter()
            .map(|item| -> Vec<InlineKeyboardButton> {
                vec![InlineKeyboardButton {
                    text: format!("📥 {item}"),
                    kind: InlineKeyboardButtonKind::CallbackData(
                        (DownloadQueryData::DownloadData {
                            book_id: book.id,
                            file_type: item.clone(),
                        })
                        .to_string(),
                    ),
                }]
            })
            .collect(),
    }
}

pub fn get_download_archive_format_keyboard(
    command: DownloadArchiveCommand,
    available_types: &[String],
) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup {
        inline_keyboard: available_types
            .iter()
            .filter(|file_type| !file_type.contains("zip"))
            .map(|file_type| {
                let callback_data = command.to_query_data(file_type.to_string()).to_string();

                vec![InlineKeyboardButton {
                    text: file_type.to_string(),
                    kind: InlineKeyboardButtonKind::CallbackData(callback_data),
                }]
            })
            .collect(),
    }
}
