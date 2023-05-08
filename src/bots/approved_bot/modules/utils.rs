use teloxide::{dptree, prelude::*, types::*};

pub trait CommandParse<T> {
    fn parse(s: &str, bot_name: &str) -> Result<T, strum::ParseError>;
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

pub enum PaginationDelta {
    OneMinus,
    OnePlus,
    FiveMinus,
    FivePlus,
}

pub trait GetPaginationCallbackData {
    fn get_pagination_callback_data(&self, target_page: u32) -> String;
}

pub fn generic_get_pagination_button<T>(
    target_page: u32,
    delta: PaginationDelta,
    callback_data: &T,
) -> InlineKeyboardButton
where
    T: GetPaginationCallbackData,
{
    let text = match delta {
        PaginationDelta::OneMinus => "<",
        PaginationDelta::OnePlus => ">",
        PaginationDelta::FiveMinus => "< 5 <",
        PaginationDelta::FivePlus => "> 5 >",
    };

    let callback_data = callback_data.get_pagination_callback_data(target_page);

    InlineKeyboardButton {
        text: text.to_string(),
        kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(callback_data),
    }
}

pub fn generic_get_pagination_keyboard<T>(
    page: u32,
    total_pages: u32,
    search_data: T,
    with_five: bool,
) -> InlineKeyboardMarkup
where
    T: GetPaginationCallbackData,
{
    let buttons: Vec<Vec<InlineKeyboardButton>> = {
        let t_page: i64 = page.into();

        let mut result: Vec<Vec<InlineKeyboardButton>> = vec![];

        let mut one_page_row: Vec<InlineKeyboardButton> = vec![];

        if t_page - 1 > 0 {
            one_page_row.push(generic_get_pagination_button(
                page - 1,
                PaginationDelta::OneMinus,
                &search_data,
            ))
        }
        if t_page < total_pages.into() {
            one_page_row.push(generic_get_pagination_button(
                page + 1,
                PaginationDelta::OnePlus,
                &search_data,
            ))
        }
        if !one_page_row.is_empty() {
            result.push(one_page_row);
        }

        if with_five {
            let mut five_page_row: Vec<InlineKeyboardButton> = vec![];
            if t_page - 5 > 0 {
                five_page_row.push(generic_get_pagination_button(
                    page - 5,
                    PaginationDelta::FiveMinus,
                    &search_data,
                ))
            }
            if t_page < total_pages.into() {
                five_page_row.push(generic_get_pagination_button(
                    page + 5,
                    PaginationDelta::FivePlus,
                    &search_data,
                ))
            }
            if !five_page_row.is_empty() {
                result.push(five_page_row);
            }
        }

        result
    };

    InlineKeyboardMarkup {
        inline_keyboard: buttons,
    }
}

pub fn split_text_to_chunks(text: &str, width: usize) -> Vec<String> {
    let mut result: Vec<String> = vec![];

    let chunks = textwrap::wrap(text, 512)
        .into_iter()
        .filter(|text| !text.replace('\r', "").is_empty())
        .map(|text| text.to_string());

    let mut index = 0;

    for val in chunks {
        if result.len() == index {
            result.push(val);
            continue;
        }

        if result[index].len() + val.len() + 1 > width {
            result.push(val);
            index += 1;
            continue;
        }

        result[index] += &format!("\n{}", &val);
    }

    result
}

#[cfg(test)]
mod tests {
    use crate::bots::approved_bot::modules::utils::split_text_to_chunks;

    #[test]
    fn test_fix_annotation_text() {
        let input = "\n Библиотека современной фантастики. Том 21\n Содержание:\n РОМАН И ПОВЕСТИ:\n Разбивая стеклянные двери… Предисловие В. Ревича\n Джон Бойнтон Пристли. Дженни Вильерс. Роман о театре. Перевод с английского В. Ашкенази\n Уильям Сароян. Тигр Тома Трейси. Повесть. Перевод с английского Р. Рыбкина\n Роберт Янг. Срубить дерево. Повесть. Перевод с английского С. Васильевой\n РАССКАЗЫ:\n Жан Рей. Рука Геца фон Берлихингена. Перевод с французского А. Григорьева\n Клод Легран. По мерке. Перевод с французского А. Григорьева\n Саке Комацу. Смерть Бикуни. Перевод с японского З. Рахима\n Ана Мария Матуте. Король Зеннов. Перевод с испанского Е. Любимовой\n Антонио Минготе. Николас. Перевод с испанского Р. Рыбкина\n Юн Бинг. Риестофер Юсеф. Перевод с норвежского Л. Жданова\n Гораций Голд. Чего стоят крылья. Перевод с английского Ф. Мендельсона\n Питер С. Бигл. Милости просим, леди Смерть! Перевод с английского Я. Евдокимовой\n Андре Майе. Как я стала писательницей. Перевод с французского Р. Рыбкина\n Джеймс Поллард. Заколдованный поезд. Перевод с английского Р. Рыбкина\n Рэй Брэдбери. Апрельское колдовство. Перевод с английского Л. Жданова\n Айзек Азимов. Небывальщина. Перевод с английского К. Сенина и В. Тальми\n Р.А. Лэфферти. Семь дней ужаса. Перевод с английского И. Почиталина\n Генри Каттнер. Сим удостоверяется… Перевод с английского К. Сенина и В. Тальми\n ";
        let expected_result: Vec<String> = vec![
            " Библиотека современной фантастики. Том 21\n Содержание:\n РОМАН И ПОВЕСТИ:\n Разбивая стеклянные двери… Предисловие В. Ревича\n Джон Бойнтон Пристли. Дженни Вильерс. Роман о театре. Перевод с английского В. Ашкенази".to_string(),
            " Уильям Сароян. Тигр Тома Трейси. Повесть. Перевод с английского Р. Рыбкина\n Роберт Янг. Срубить дерево. Повесть. Перевод с английского С. Васильевой\n РАССКАЗЫ:\n Жан Рей. Рука Геца фон Берлихингена. Перевод с французского А. Григорьева".to_string(),
            " Клод Легран. По мерке. Перевод с французского А. Григорьева\n Саке Комацу. Смерть Бикуни. Перевод с японского З. Рахима\n Ана Мария Матуте. Король Зеннов. Перевод с испанского Е. Любимовой\n Антонио Минготе. Николас. Перевод с испанского Р. Рыбкина".to_string(),
            " Юн Бинг. Риестофер Юсеф. Перевод с норвежского Л. Жданова\n Гораций Голд. Чего стоят крылья. Перевод с английского Ф. Мендельсона\n Питер С. Бигл. Милости просим, леди Смерть! Перевод с английского Я. Евдокимовой\n Андре Майе. Как я стала писательницей. Перевод с французского Р. Рыбкина".to_string(),
            " Джеймс Поллард. Заколдованный поезд. Перевод с английского Р. Рыбкина\n Рэй Брэдбери. Апрельское колдовство. Перевод с английского Л. Жданова\n Айзек Азимов. Небывальщина. Перевод с английского К. Сенина и В. Тальми\n Р.А. Лэфферти. Семь дней ужаса. Перевод с английского И. Почиталина".to_string(),
            " Генри Каттнер. Сим удостоверяется… Перевод с английского К. Сенина и В. Тальми".to_string()
        ];

        let result = split_text_to_chunks(input, 512);

        assert_eq!(result, expected_result);
    }
}
