pub fn format_registered_message(username: &str) -> String {
    format!("@{username} зарегистрирован и через несколько минут будет подключен!")
}

pub const MAY_BE_ALREADY_REGISTERED: &str = "Ошибка! Возможно бот уже зарегистрирован!";

pub const ERROR_MESSAGE: &str = "Ошибка! Что-то не так с ботом!";

pub const LIMIT_EXTENDED_MESSAGE: &str = "Вы достигли максимального количества ботов!";

pub const ALREADY_EXISTS_MESSAGE: &str = "Ошибка! Бот с таким токеном уже зарегистрирован!";
