use crate::bots::BotHandlerInternal;

use teloxide::{prelude::*, utils::command::BotCommands};

#[derive(BotCommands, Clone)]
#[command(rename = "lowercase")]
enum HelpCommand {
    Start,
    Help,
}

pub async fn help_handler(message: Message, bot: AutoSend<Bot>) -> BotHandlerInternal {
    let name = message
        .from()
        .map(|user| user.first_name.clone())
        .unwrap_or("пользователь".to_string());

    match bot
        .send_message(
            message.chat.id,
            format!(
                "
Привет, {name}! \n
Этот бот поможет тебе загружать книги.\n
Настройки языков для поиска /settings.\n
\n\n
Регистрация своего бота: \n
1. Зарегистрируй бота в @BotFather . \n
2. И перешли сюда сообщение об успешной регистрации.
(Начинается с: Done! Congratulations on your new bot.)
        "
            ),
        )
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
