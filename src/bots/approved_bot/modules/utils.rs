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
        if t_page + 1 <= total_pages.into() {
            one_page_row.push(generic_get_pagination_button(
                page + 1,
                PaginationDelta::OnePlus,
                &search_data,
            ))
        }
        if one_page_row.len() != 0 {
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
            if t_page + 1 <= total_pages.into() {
                five_page_row.push(generic_get_pagination_button(
                    page + 5,
                    PaginationDelta::FivePlus,
                    &search_data,
                ))
            }
            if five_page_row.len() != 0 {
                result.push(five_page_row);
            }
        }

        result
    };

    InlineKeyboardMarkup {
        inline_keyboard: buttons,
    }
}
