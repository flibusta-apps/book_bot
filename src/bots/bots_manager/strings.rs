pub fn format_registered_message(username: &str) -> String {
    return format!("@{username} зарегистрирован и через несколько минут будет подключен!", username = username);
}

pub const ALREADY_REGISTERED: &str= "Ошибка! Возможно бот уже зарегистрирован!";

pub const ERROR_MESSAGE: &str = "Ошибка! Что-то не так с ботом!";
