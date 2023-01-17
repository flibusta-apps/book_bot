pub const HELP_MESSAGE: &str = "
Зарегистрируй бота в @BotFather .
И перешли сюда сообщение об успешной регистрации.
(Начинается с: Done! Congratulations on your new bot.)
";

pub fn format_registered_message(username: &str) -> String {
    return format!("@{username} зарегистрирован и через несколько минут будет подключен!", username = username);
}

pub const ALREADY_REGISTERED: &str= "Ошибка! Возможно бот уже зарегистрирован!";

pub const ERROR_MESSAGE: &str = "Ошибка! Что-то не так с ботом!";

pub const BOT_REGISTERED_TO_ADMIN: &str = "Новый бот зарегистрирован!";
