use teloxide::{dptree, prelude::*, types::*};

use super::errors::CommandParseError;

pub trait CommandParse<T> {
    fn parse(s: &str) -> Result<T, CommandParseError>;
}

pub fn strip_bot_mention(text: &str, bot_name: &str) -> String {
    if bot_name.is_empty() {
        return text.to_string();
    }

    let mention = format!("@{bot_name}");
    let lower_text = text.to_ascii_lowercase();
    let lower_mention = mention.to_ascii_lowercase();

    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    let mut lower_rest = lower_text.as_str();

    while let Some(pos) = lower_rest.find(&lower_mention) {
        result.push_str(&rest[..pos]);
        rest = &rest[pos + mention.len()..];
        lower_rest = &lower_rest[pos + mention.len()..];
    }
    result.push_str(rest);

    result
}

pub fn filter_command<Output>() -> crate::bots::BotHandler
where
    Output: CommandParse<Output> + Send + Sync + 'static,
{
    dptree::entry().chain(dptree::filter_map(move |message: Message, me: Me| {
        let bot_name = me.user.username.unwrap_or_default();
        message.text().and_then(|text| {
            let normalized = strip_bot_mention(text, &bot_name);
            Output::parse(&normalized).ok()
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::strip_bot_mention;

    #[test]
    fn strips_matching_case() {
        assert_eq!(strip_bot_mention("/d_1@MyBot", "MyBot"), "/d_1");
    }

    #[test]
    fn strips_case_insensitive_lower_bot_name() {
        assert_eq!(strip_bot_mention("/d_1@MyBot", "mybot"), "/d_1");
    }

    #[test]
    fn strips_case_insensitive_lower_mention() {
        assert_eq!(strip_bot_mention("/d_1@mybot", "MyBot"), "/d_1");
    }

    #[test]
    fn leaves_text_without_mention_unchanged() {
        assert_eq!(strip_bot_mention("/d_1", "MyBot"), "/d_1");
    }

    #[test]
    fn empty_bot_name_is_a_no_op() {
        assert_eq!(strip_bot_mention("/d_1@MyBot", ""), "/d_1@MyBot");
    }

    #[test]
    fn strips_every_occurrence() {
        assert_eq!(
            strip_bot_mention("/d_1@MyBot text @MyBot more", "mybot"),
            "/d_1 text  more"
        );
    }
}
