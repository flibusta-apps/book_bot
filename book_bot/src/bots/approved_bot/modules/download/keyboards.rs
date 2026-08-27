use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::bots::approved_bot::services::book_library::types::Book;

use super::{
    callback_data::{CheckArchiveStatus, DownloadQueryData},
    commands::DownloadArchiveCommand,
};

pub fn get_check_keyboard(task_id: String) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        "Обновить статус",
        (CheckArchiveStatus { task_id }).to_string(),
    )]])
}

pub fn get_download_format_keyboard(book: &Book) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(
        book.available_types
            .iter()
            .map(|item| -> Vec<InlineKeyboardButton> {
                vec![InlineKeyboardButton::callback(
                    format!("📥 {item}"),
                    (DownloadQueryData::DownloadData {
                        book_id: book.id,
                        file_type: item.clone(),
                    })
                    .to_string(),
                )]
            })
            .collect::<Vec<_>>(),
    )
}

pub fn get_download_archive_format_keyboard(
    command: DownloadArchiveCommand,
    available_types: &[String],
) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(
        available_types
            .iter()
            .filter(|file_type| !file_type.contains("zip"))
            .map(|file_type| {
                let callback_data = command.to_query_data(file_type.to_string()).to_string();

                vec![InlineKeyboardButton::callback(
                    file_type.to_string(),
                    callback_data,
                )]
            })
            .collect::<Vec<_>>(),
    )
}
