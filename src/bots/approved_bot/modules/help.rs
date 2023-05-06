use crate::bots::BotHandlerInternal;

use teloxide::{prelude::*, utils::command::BotCommands, types::ParseMode, adaptors::Throttle};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum HelpCommand {
    Start,
    Help,
}


pub async fn help_handler(message: Message, bot: Throttle<Bot>) -> BotHandlerInternal {
    let name = message
        .from()
        .map(|user| user.first_name.clone())
        .unwrap_or("пользователь".to_string());

    match bot
        .send_message(
            message.chat.id,
            format!(
                "
Привет, {name}!

Этот бот поможет тебе загружать книги.

Настройки языков для поиска /settings.

Регистрация своего бота:
1. <a href=\"https://telegra.ph/Registraciya-svoego-bota-01-24\">Зарегистрируй бота</a> в @BotFather.
2. И перешли сюда сообщение об успешной регистрации.
(Начинается с: Done! Congratulations on your new bot.)
        "
            ),
        )
        .parse_mode(ParseMode::Html)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(Box::new(err)),
    }
}

pub fn get_help_handler() -> crate::bots::BotHandler {
    dptree::entry().branch(
        Update::filter_message().branch(
            dptree::entry()
                .filter_command::<HelpCommand>()
                .endpoint(|message, bot| async move { help_handler(message, bot).await }),
        ),
    )
}
