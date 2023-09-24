use crate::bots::BotHandlerInternal;

use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    utils::command::BotCommands,
};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum SupportCommand {
    Support,
    Donate,
}

pub async fn support_command_handler(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
) -> BotHandlerInternal {
    let is_bot = message.from().unwrap().is_bot;

    let username = if is_bot {
        &message
            .reply_to_message()
            .unwrap()
            .from()
            .unwrap()
            .first_name
    } else {
        &message.from().unwrap().first_name
    };

    let message_text = format!(
        "
Привет, {username}!

Этот бот существует благодаря пожертвованиям от наших пользователей.
Однако, для его дальнейшего развития и поддержки серверов требуются финансовые средства.
Буду очень благодарен за любую сумму пожертвования!

Спасибо!

Тинькофф:
<pre>5536913820619688</pre>

Сбербанк:
<pre>+79534966556</pre>

Paypal:
<a href=\"https://www.paypal.me/kurbezz\">@kurbezz</a>

TRON (TRC20) - USDT:
<pre>TYnWyK7mJhasjuCGYYyZxqQ1VUDxgZfuRi</pre>

Bitcoin - BTC:
<pre>12g9XY6oqLwaw7V9LJnLanxLNNKjJRbWUH</pre>

The Open Network - TON:
<pre>UQA4MySrq_60b_VMlR6UEmc_0u-neAUTXdtv8oKr_i6uhQNd</pre>
"
    );

    bot.send_message(message.chat.id, message_text)
        .parse_mode(teloxide::types::ParseMode::Html)
        .disable_web_page_preview(true)
        .await?;

    Ok(())
}

pub fn get_support_handler() -> crate::bots::BotHandler {
    dptree::entry().branch(
        Update::filter_message().branch(
            dptree::entry()
                .filter_command::<SupportCommand>()
                .endpoint(support_command_handler),
        ),
    )
}
