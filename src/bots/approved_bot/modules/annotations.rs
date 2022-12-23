use std::{convert::TryInto, str::FromStr};

use futures::TryStreamExt;
use regex::Regex;
use teloxide::{dispatching::UpdateFilterExt, dptree, prelude::*, types::*};
use tokio_util::compat::FuturesAsyncReadCompatExt;

use crate::bots::{
    approved_bot::{
        modules::utils::generic_get_pagination_keyboard,
        services::book_library::{
            get_author_annotation, get_book_annotation,
            types::{AuthorAnnotation, BookAnnotation},
        },
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};

use super::utils::{filter_command, CommandParse, GetPaginationCallbackData};

#[derive(Clone)]
pub enum AnnotationCommand {
    Book { id: u32 },
    Author { id: u32 },
}

impl CommandParse<Self> for AnnotationCommand {
    fn parse(s: &str, bot_name: &str) -> Result<Self, strum::ParseError> {
        let re = Regex::new(r"^/(?P<an_type>a|b)_an_(?P<id>\d+)$")
            .unwrap_or_else(|_| panic!("Can't create AnnotationCommand regexp!"));

        let full_bot_name = format!("@{bot_name}");
        let after_replace = s.replace(&full_bot_name, "");

        let caps = re.captures(&after_replace);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let annotation_type = &caps["an_type"];
        let id: u32 = caps["id"]
            .parse()
            .unwrap_or_else(|_| panic!("Can't get id from AnnotationCommand!"));

        match annotation_type {
            "a" => Ok(AnnotationCommand::Author { id }),
            "b" => Ok(AnnotationCommand::Book { id }),
            _ => Err(strum::ParseError::VariantNotFound),
        }
    }
}

#[derive(Clone)]
pub enum AnnotationCallbackData {
    Book { id: u32, page: u32 },
    Author { id: u32, page: u32 },
}

impl FromStr for AnnotationCallbackData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"^(?P<an_type>a|b)_an_(?P<id>\d+)_(?P<page>\d+)$").unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let annotation_type = &caps["an_type"];
        let id = caps["id"].parse::<u32>().unwrap();
        let page = caps["page"].parse::<u32>().unwrap();

        match annotation_type {
            "a" => Ok(AnnotationCallbackData::Author { id, page }),
            "b" => Ok(AnnotationCallbackData::Book { id, page }),
            _ => Err(strum::ParseError::VariantNotFound),
        }
    }
}

impl ToString for AnnotationCallbackData {
    fn to_string(&self) -> String {
        match self {
            AnnotationCallbackData::Book { id, page } => format!("b_an_{id}_{page}"),
            AnnotationCallbackData::Author { id, page } => format!("a_an_{id}_{page}"),
        }
    }
}

pub trait AnnotationFormat {
    fn get_file(&self) -> Option<&String>;
    fn get_text(&self) -> &str;

    fn is_normal_text(&self) -> bool;
}

impl AnnotationFormat for BookAnnotation {
    fn get_file(&self) -> Option<&String> {
        self.file.as_ref()
    }

    fn get_text(&self) -> &str {
        self.text.as_str()
    }

    fn is_normal_text(&self) -> bool {
        self.text.replace('\n', "").replace(' ', "").len() != 0
    }
}

impl GetPaginationCallbackData for AnnotationCallbackData {
    fn get_pagination_callback_data(&self, target_page: u32) -> String {
        match self {
            AnnotationCallbackData::Book { id, .. } => AnnotationCallbackData::Book {
                id: id.clone(),
                page: target_page,
            },
            AnnotationCallbackData::Author { id, .. } => AnnotationCallbackData::Author {
                id: id.clone(),
                page: target_page,
            },
        }
        .to_string()
    }
}

impl AnnotationFormat for AuthorAnnotation {
    fn get_file(&self) -> Option<&String> {
        self.file.as_ref()
    }

    fn get_text(&self) -> &str {
        self.text.as_str()
    }

    fn is_normal_text(&self) -> bool {
        self.text.replace('\n', "").replace(' ', "").len() != 0
    }
}

async fn download_image(
    file: &String,
) -> Result<reqwest::Response, Box<dyn std::error::Error + Send + Sync>> {
    let response = reqwest::get(file).await;

    let response = match response {
        Ok(v) => v,
        Err(err) => return Err(Box::new(err)),
    };

    let response = match response.error_for_status() {
        Ok(v) => v,
        Err(err) => return Err(Box::new(err)),
    };

    Ok(response)
}

pub async fn send_annotation_handler<T, Fut>(
    message: Message,
    bot: AutoSend<Bot>,
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

    let annotation = match annotation_getter(id).await {
        Ok(v) => v,
        Err(err) => return Err(err),
    };

    if annotation.get_file().is_none() && !annotation.is_normal_text() {
        return match bot
            .send_message(message.chat.id, "Аннотация недоступна :(")
            .reply_to_message_id(message.id)
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

            log::info!("{}", file);

            match bot
                .send_photo(message.chat.id, InputFile::read(data))
                .send()
                .await
            {
                Ok(_) => (),
                Err(err) => log::info!("{}", err),
            }
        }
    };

    if !annotation.is_normal_text() {
        return Ok(());
    }

    let chunked_text: Vec<String> = textwrap::wrap(annotation.get_text(), 512)
        .into_iter()
        .filter(|text| text.replace('\r', "").len() != 0)
        .map(|text| text.to_string())
        .collect();
    let current_text = chunked_text.get(0).unwrap();

    let callback_data = match command {
        AnnotationCommand::Book { id } => AnnotationCallbackData::Book { id, page: 1 },
        AnnotationCommand::Author { id } => AnnotationCallbackData::Author { id, page: 1 },
    };
    let keyboard = generic_get_pagination_keyboard(
        1,
        chunked_text.len().try_into().unwrap(),
        callback_data,
        false,
    );

    match bot
        .send_message(message.chat.id, current_text)
        .reply_markup(keyboard)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(Box::new(err)),
    }
}

pub async fn annotation_pagination_handler<T, Fut>(
    cq: CallbackQuery,
    bot: AutoSend<Bot>,
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

    let annotation = match annotation_getter(id).await {
        Ok(v) => v,
        Err(err) => return Err(err),
    };

    let message = match cq.message {
        Some(v) => v,
        None => return Ok(()),
    };

    let page_index: usize = page.try_into().unwrap();
    let chunked_text: Vec<String> = textwrap::wrap(annotation.get_text(), 512)
        .into_iter()
        .filter(|text| text.replace('\r', "").len() != 0)
        .map(|text| text.to_string())
        .collect();
    let current_text = chunked_text.get(page_index - 1).unwrap();

    let keyboard = generic_get_pagination_keyboard(
        page,
        chunked_text.len().try_into().unwrap(),
        callback_data,
        false,
    );

    match bot
        .edit_message_text(message.chat.id, message.id, current_text)
        .reply_markup(keyboard)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(Box::new(err)),
    }
}

pub fn get_annotations_handler() -> crate::bots::BotHandler {
    dptree::entry()
        .branch(
            Update::filter_message()
                .chain(filter_command::<AnnotationCommand>())
                .endpoint(
                    |message: Message, bot: AutoSend<Bot>, command: AnnotationCommand| async move {
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
                     bot: AutoSend<Bot>,
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
