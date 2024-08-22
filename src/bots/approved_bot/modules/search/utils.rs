use teloxide::types::{CallbackQuery, MaybeInaccessibleMessage};


pub fn get_query(cq: CallbackQuery) -> Option<String> {
    match cq.message {
        Some(message) => {
            match message {
                MaybeInaccessibleMessage::Regular(message) => {
                    message
                        .text()
                        .map_or(None, |text| Some(text.replace(['/', '&', '?'], "")))
                }
                MaybeInaccessibleMessage::Inaccessible(_) => None,
            }
        }
        None => None,

    }
}
