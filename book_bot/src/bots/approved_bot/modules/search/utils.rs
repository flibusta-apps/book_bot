use teloxide::types::{CallbackQuery, MaybeInaccessibleMessage};

pub fn get_query(cq: CallbackQuery) -> Option<String> {
    match cq.message {
        Some(message) => match message {
            MaybeInaccessibleMessage::Regular(message) => match message.reply_to_message() {
                Some(reply_to_message) => reply_to_message
                    .text()
                    .map(|text| text.replace(['/', '&', '?'], "")),
                None => None,
            },
            MaybeInaccessibleMessage::Inaccessible(_) => None,
        },
        None => None,
    }
}
