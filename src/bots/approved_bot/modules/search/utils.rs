use teloxide::types::CallbackQuery;

pub fn get_query(cq: CallbackQuery) -> Option<String> {
    cq.message
        .map(|message| {
            message
                .reply_to_message()
                .map(|reply_to_message| {
                    reply_to_message
                        .text()
                        .map(|text| text.replace(['/', '&', '?'], ""))
                })
                .unwrap_or(None)
        })
        .unwrap_or(None)
}
