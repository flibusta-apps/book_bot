pub mod callback_data;
pub mod commands;
pub mod errors;
pub mod formatter;

use std::convert::TryInto;

use futures::TryStreamExt;

use teloxide::{
    adaptors::{CacheMe, Throttle},
    dispatching::UpdateFilterExt,
    dptree,
    prelude::*,
    types::*,
};
use tokio_util::compat::FuturesAsyncReadCompatExt;

use crate::bots::{
    approved_bot::{
        modules::utils::pagination::generic_get_pagination_keyboard,
        services::book_library::{get_author_annotation, get_book_annotation},
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};

use self::{
    callback_data::AnnotationCallbackData, commands::AnnotationCommand,
    errors::AnnotationFormatError, formatter::AnnotationFormat,
};

use super::utils::{filter_command::filter_command, split_text::split_text_to_chunks};

async fn download_image(
    file: &String,
) -> Result<reqwest::Response, Box<dyn std::error::Error + Send + Sync>> {
    Ok(reqwest::get(file).await?.error_for_status()?)
}

pub async fn send_annotation_handler<T, Fut>(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
    command: AnnotationCommand,
    annotation_getter: fn(id: u32) -> Fut,
) -> BotHandlerInternal
where
    T: AnnotationFormat,
    Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>>,
{
    let id = match command {
        AnnotationCommand::Book { id } => id,
        AnnotationCommand::Author { id } => id,
    };

    let annotation = annotation_getter(id).await?;

    if annotation.get_file().is_none() && !annotation.is_normal_text() {
        return match bot
            .send_message(message.chat.id, "Аннотация недоступна :(")
            .reply_parameters(ReplyParameters::new(message.id))
            .send()
            .await
        {
            Ok(_) => Ok(()),
            Err(err) => Err(Box::new(err)),
        };
    };

    if let Some(file) = annotation.get_file() {
        let image_response = download_image(file).await;

        if let Ok(v) = image_response {
            let data = v
                .bytes_stream()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                .into_async_read()
                .compat();

            #[allow(unused_must_use)]
            {
                bot.send_photo(message.chat.id, InputFile::read(data))
                    .send()
                    .await;
            }
        }
    };

    if !annotation.is_normal_text() {
        return Err(Box::new(AnnotationFormatError {
            command,
            text: annotation.get_text().to_string(),
        }));
    }

    let annotation_text = annotation.get_text();
    let chunked_text = split_text_to_chunks(annotation_text, 512);
    let current_text = chunked_text.first().unwrap();

    let callback_data = match command {
        AnnotationCommand::Book { id } => AnnotationCallbackData::Book { id, page: 1 },
        AnnotationCommand::Author { id } => AnnotationCallbackData::Author { id, page: 1 },
    };
    let keyboard =
        generic_get_pagination_keyboard(1, chunked_text.len().try_into()?, callback_data, false);

    bot.send_message(message.chat.id, current_text)
        .reply_markup(keyboard)
        .send()
        .await?;

    Ok(())
}

pub async fn annotation_pagination_handler<T, Fut>(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    callback_data: AnnotationCallbackData,
    annotation_getter: fn(id: u32) -> Fut,
) -> BotHandlerInternal
where
    T: AnnotationFormat,
    Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>>,
{
    let (id, page) = match callback_data {
        AnnotationCallbackData::Book { id, page } => (id, page),
        AnnotationCallbackData::Author { id, page } => (id, page),
    };

    let annotation = annotation_getter(id).await?;

    let message = match cq.message {
        Some(v) => v,
        None => return Ok(()),
    };

    let request_page: usize = page.try_into().unwrap();

    let annotation_text = annotation.get_text();
    let chunked_text = split_text_to_chunks(annotation_text, 512);

    let page_index = if request_page <= chunked_text.len() {
        request_page
    } else {
        chunked_text.len()
    };
    let current_text = chunked_text.get(page_index - 1).unwrap();

    let keyboard =
        generic_get_pagination_keyboard(page, chunked_text.len().try_into()?, callback_data, false);

    bot.edit_message_text(message.chat().id, message.id(), current_text)
        .reply_markup(keyboard)
        .send()
        .await?;

    Ok(())
}

pub fn get_annotations_handler() -> crate::bots::BotHandler {
    dptree::entry()
        .branch(
            Update::filter_message()
                .chain(filter_command::<AnnotationCommand>())
                .endpoint(
                    |message: Message, bot: CacheMe<Throttle<Bot>>, command: AnnotationCommand| async move {
                        match command {
                            AnnotationCommand::Book { .. } => {
                                send_annotation_handler(message, bot, command, get_book_annotation)
                                    .await
                            }
                            AnnotationCommand::Author { .. } => {
                                send_annotation_handler(
                                    message,
                                    bot,
                                    command,
                                    get_author_annotation,
                                )
                                .await
                            }
                        }
                    },
                ),
        )
        .branch(
            Update::filter_callback_query()
                .chain(filter_callback_query::<AnnotationCallbackData>())
                .endpoint(
                    |cq: CallbackQuery,
                     bot: CacheMe<Throttle<Bot>>,
                     callback_data: AnnotationCallbackData| async move {
                        match callback_data {
                            AnnotationCallbackData::Book { .. } => {
                                annotation_pagination_handler(
                                    cq,
                                    bot,
                                    callback_data,
                                    get_book_annotation,
                                )
                                .await
                            }
                            AnnotationCallbackData::Author { .. } => {
                                annotation_pagination_handler(
                                    cq,
                                    bot,
                                    callback_data,
                                    get_author_annotation,
                                )
                                .await
                            }
                        }
                    },
                ),
        )
}
