use teloxide::types::{CallbackQuery, MaybeInaccessibleMessage};

pub fn get_query(cq: &CallbackQuery) -> Option<String> {
    match &cq.message {
        Some(message) => match message {
            MaybeInaccessibleMessage::Regular(message) => match message.reply_to_message() {
                Some(reply_to_message) => reply_to_message.text().map(str::to_owned),
                None => None,
            },
            MaybeInaccessibleMessage::Inaccessible(_) => None,
        },
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::get_query;
    use teloxide::types::{CallbackQuery, MaybeInaccessibleMessage};

    fn make_cq(message: Option<MaybeInaccessibleMessage>) -> CallbackQuery {
        serde_json::from_value(serde_json::json!({
            "id": "1",
            "from": {
                "id": 1,
                "is_bot": false,
                "first_name": "T"
            },
            "chat_instance": "1",
            "message": message,
        }))
        .unwrap()
    }

    #[test]
    fn preserves_search_punctuation_on_subsequent_pages() {
        let message = serde_json::from_value(serde_json::json!({
            "message_id": 2, "date": 1,
            "chat": {"id": 1, "type": "private", "first_name": "T"},
            "text": "results", "reply_to_message": {
                "message_id": 1, "date": 1,
                "chat": {"id": 1, "type": "private", "first_name": "T"},
                "text": "a/b & c?"
            }
        }))
        .unwrap();
        assert_eq!(
            get_query(&make_cq(Some(message))).as_deref(),
            Some("a/b & c?")
        );
    }

    #[test]
    fn returns_none_when_message_missing() {
        let cq = make_cq(None);
        assert_eq!(get_query(&cq), None);
    }
}
