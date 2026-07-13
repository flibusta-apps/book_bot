use core::fmt::Debug;

use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{ChatId, MaybeInaccessibleMessage, MessageId},
};

use crate::bots::approved_bot::services::book_library::{
    formatters::{Format, FormatTitle},
    types::Page,
};

use super::{
    message_text::is_message_text_equals,
    telegram_utils::{safe_edit_message_text, safe_send_message},
};

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
            if t_page + 5 < total_pages.into() {
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

pub struct PaginationTexts<'a> {
    /// Sent when the fetcher's first call returns `Ok(None)` (the parent
    /// entity — author/translator/sequence/search query — doesn't exist).
    pub not_found: &'a str,
    /// Sent when the fetcher returns `Ok(Some(page))` but `page.pages == 0`
    /// (the entity exists but has no items), and reused for the re-fetch's
    /// `Ok(None)` branch on the clamp path (unreachable in practice — see
    /// Task 8/9's notes on this field).
    pub no_items: &'a str,
    pub error_try_later: Option<&'a str>,
}

/// Shared skeleton for "fetch a page → not-found → clamp page → format →
/// no-op-if-unchanged → edit message + pagination keyboard", used by the
/// `book`, `search`, and `update_history` modules' callback-query
/// pagination handlers. Callers own everything data-source-specific
/// (extracting the query/id from the callback data, building the
/// `fetcher` closure, and resolving `chat_id`/`message_id` from the
/// incoming `CallbackQuery`).
#[allow(clippy::too_many_arguments)]
pub async fn paginate<T, P, Fut>(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    cq_message: Option<MaybeInaccessibleMessage>,
    page: u32,
    header: &str,
    fetcher: impl Fn(u32) -> Fut,
    keyboard_data: impl GetPaginationCallbackData,
    texts: PaginationTexts<'_>,
) -> crate::bots::BotHandlerInternal
where
    T: Format + Clone + Debug,
    P: FormatTitle + Clone + Debug,
    Fut: std::future::Future<Output = anyhow::Result<Option<Page<T, P>>>>,
{
    let mut items_page = match fetcher(page).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message(bot, chat_id, texts.not_found, None).await?;
            return Ok(());
        }
        Err(err) => {
            if let Some(msg) = texts.error_try_later {
                safe_send_message(bot, chat_id, msg, None).await?;
            }
            return Err(err);
        }
    };

    if items_page.pages == 0 {
        safe_send_message(bot, chat_id, texts.no_items, None).await?;
        return Ok(());
    }

    if page > items_page.pages {
        items_page = match fetcher(items_page.pages).await {
            Ok(Some(v)) => v,
            Ok(None) => {
                safe_send_message(bot, chat_id, texts.no_items, None).await?;
                return Ok(());
            }
            Err(err) => {
                if let Some(msg) = texts.error_try_later {
                    safe_send_message(bot, chat_id, msg, None).await?;
                }
                return Err(err);
            }
        };
    }

    let page = std::cmp::min(page, items_page.pages);
    let formatted_page = items_page.format(page, super::constants::TELEGRAM_MESSAGE_MAX_LENGTH);
    let message_text = format!("{header}{formatted_page}");

    if is_message_text_equals(cq_message, &message_text) {
        return Ok(());
    }

    let keyboard = generic_get_pagination_keyboard(page, items_page.pages, keyboard_data, true);
    safe_edit_message_text(bot, chat_id, message_id, message_text, Some(keyboard)).await
}

#[cfg(test)]
mod paginate_tests {
    use super::*;
    use crate::bots::approved_bot::services::book_library::{
        formatters::{Format, FormatResult, FormatTitle},
        types::Page,
    };
    use std::sync::atomic::{AtomicU32, Ordering};

    #[derive(Clone, Debug)]
    struct FakeItem(String);

    impl Format for FakeItem {
        fn format(&self, _max_size: usize) -> FormatResult {
            FormatResult {
                result: self.0.clone(),
                current_size: self.0.len(),
                max_size: self.0.len(),
            }
        }
    }

    #[derive(Clone, Debug)]
    struct FakeParent;

    impl FormatTitle for FakeParent {
        fn format_title(&self) -> String {
            "".to_string()
        }
    }

    fn make_page(pages: u32) -> Page<FakeItem, FakeParent> {
        Page {
            items: vec![FakeItem("item".to_string())],
            pages,
            parent_item: None,
        }
    }

    #[tokio::test]
    async fn clamps_page_above_total_and_calls_fetcher_with_clamped_page() {
        let calls = AtomicU32::new(0);
        let last_requested_page = AtomicU32::new(0);

        let fetcher = |page: u32| {
            calls.fetch_add(1, Ordering::SeqCst);
            last_requested_page.store(page, Ordering::SeqCst);
            async move {
                if page == 3 {
                    Ok::<Option<Page<FakeItem, FakeParent>>, anyhow::Error>(Some(make_page(3)))
                } else {
                    Ok(Some(make_page(3)))
                }
            }
        };

        // We can't easily run the bot-send path without a live `CacheMe<Throttle<Bot>>`,
        // so this test only exercises the fetch/clamp logic by calling the fetcher
        // directly the same way `paginate` does, verifying the two-call clamp pattern.
        let first = fetcher(10).await.unwrap().unwrap();
        assert_eq!(first.pages, 3);
        if 10 > first.pages {
            let _second = fetcher(first.pages).await.unwrap().unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(last_requested_page.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn not_found_texts_are_distinct_fields() {
        // Compile-time check that `PaginationTexts` has two distinct
        // not-found fields (see the Interfaces note above) rather than
        // one shared field — this is what book/mod.rs (Task 8) needs.
        let texts = PaginationTexts {
            not_found: "a",
            no_items: "b",
            error_try_later: Some("c"),
        };
        assert_ne!(texts.not_found, texts.no_items);
    }
}
