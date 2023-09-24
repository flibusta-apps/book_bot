use regex::Regex;

pub fn get_token(message_text: &str) -> Option<&str> {
    let re = Regex::new("(?P<token>[0-9]+:[0-9a-zA-Z-_]+)").unwrap();

    match re.find(message_text) {
        Some(v) => Some(v.as_str()),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_token_fail() {
        let message = "wrong_token";

        let result = get_token(message);

        assert!(result.is_none())
    }

    #[test]
    fn check_token_short() {
        let message = "
        Done! Congratulations on your new bot. You will find it at t.me/aaaa_bot.
        You can now add a description, about section and profile picture for your bot,
        see /help for a list of commands.
        By the way, when you've finished creating your cool bot, ping our Bot Support if you want a better username for it.
        Just make sure the bot is fully operational before you do this. \
        \
        Use this token to access the HTTP API: \
        5555555555:AAF-AAAAAAAA1239AA2AAsvy13Axp23RAa \
        Keep your token secure and store it safely, it can be used by anyone to control your bot. \
        \
        For a description of the Bot API, see this page: https://core.telegram.org/bots/api \
        ";

        let result = get_token(message);

        assert_eq!(
            result.unwrap(),
            "5555555555:AAF-AAAAAAAA1239AA2AAsvy13Axp23RAa"
        );
    }

    #[test]
    fn check_token_long() {
        let message = "5555555555:AAF-AAAAAAAA1239AA2AAsvy13Axp23RAa";

        let result = get_token(message);

        assert_eq!(result.unwrap(), message);
    }
}
