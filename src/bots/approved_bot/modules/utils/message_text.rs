use teloxide::types::*;

pub fn is_message_text_equals(message: Option<MaybeInaccessibleMessage>, text: &str) -> bool {
    let message = match message {
        Some(v) => v,
        None => return false,
    };

    let message = match message {
        MaybeInaccessibleMessage::Inaccessible(_) => return false,
        MaybeInaccessibleMessage::Regular(v) => v,
    };

    match message.text() {
        Some(msg_text) => text == msg_text,
        None => false,
    }
}
