use teloxide::{dptree, prelude::*, types::*};

use super::errors::CommandParseError;

pub trait CommandParse<T> {
    fn parse(s: &str, bot_name: &str) -> Result<T, CommandParseError>;
}

pub fn filter_command<Output>() -> crate::bots::BotHandler
where
    Output: CommandParse<Output> + Send + Sync + 'static,
{
    dptree::entry().chain(dptree::filter_map(move |message: Message, me: Me| {
        let bot_name = me.user.username.expect("Bots must have a username");
        message
            .text()
            .and_then(|text| Output::parse(text, &bot_name).ok())
    }))
}
