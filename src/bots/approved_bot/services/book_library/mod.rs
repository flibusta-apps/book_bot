pub mod formatters;
pub mod types;

use once_cell::sync::Lazy;
use smartstring::alias::String as SmartString;

use serde::de::DeserializeOwned;
use smallvec::SmallVec;
use tracing::log;

use crate::config;

use self::types::Empty;


pub static CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);


fn get_allowed_langs_params(
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> Vec<(&'static str, SmartString)> {
    allowed_langs
        .into_iter()
        .map(|lang| ("allowed_langs", lang))
        .collect()
}

async fn _make_request<T>(
    url: &str,
    params: Vec<(&str, SmartString)>,
) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
where
    T: DeserializeOwned,
{
    let response = CLIENT
        .get(format!("{}{}", &config::CONFIG.book_server_url, url))
        .query(&params)
        .header("Authorization", &config::CONFIG.book_server_api_key)
        .send()
        .await?
        .error_for_status()?;

    match response.json::<T>().await {
        Ok(v) => Ok(v),
        Err(err) => {
            log::error!("Failed serialization: url={:?} err={:?}", url, err);
            Err(Box::new(err))
        }
    }
}

pub async fn get_book(id: u32) -> Result<types::Book, Box<dyn std::error::Error + Send + Sync>> {
    _make_request(&format!("/api/v1/books/{id}"), vec![]).await
}

pub async fn get_random_book_by_genre(
    allowed_langs: SmallVec<[SmartString; 3]>,
    genre: Option<u32>,
) -> Result<types::Book, Box<dyn std::error::Error + Send + Sync>> {
    let mut params = get_allowed_langs_params(allowed_langs);

    if let Some(v) = genre {
        params.push(("genre", v.to_string().into()));
    }

    _make_request("/api/v1/books/random", params).await
}

pub async fn get_random_book(
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> Result<types::Book, Box<dyn std::error::Error + Send + Sync>> {
    get_random_book_by_genre(allowed_langs, None).await
}

pub async fn get_random_author(
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> Result<types::Author, Box<dyn std::error::Error + Send + Sync>> {
    let params = get_allowed_langs_params(allowed_langs);

    _make_request("/api/v1/authors/random", params).await
}

pub async fn get_random_sequence(
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> Result<types::Sequence, Box<dyn std::error::Error + Send + Sync>> {
    let params = get_allowed_langs_params(allowed_langs);

    _make_request("/api/v1/sequences/random", params).await
}

pub async fn get_genre_metas() -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    _make_request("/api/v1/genres/metas", vec![]).await
}

pub async fn get_genres(
    meta: SmartString,
) -> Result<types::Page<types::Genre, Empty>, Box<dyn std::error::Error + Send + Sync>> {
    let params = vec![("meta", meta)];

    _make_request("/api/v1/genres", params).await
}

const PAGE_SIZE: &str = "5";

pub async fn search_book(
    query: String,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> Result<types::Page<types::SearchBook, Empty>, Box<dyn std::error::Error + Send + Sync>> {
    let mut params = get_allowed_langs_params(allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(format!("/api/v1/books/search/{query}").as_str(), params).await
}

pub async fn search_author(
    query: String,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> Result<types::Page<types::Author, Empty>, Box<dyn std::error::Error + Send + Sync>> {
    let mut params = get_allowed_langs_params(allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(format!("/api/v1/authors/search/{query}").as_str(), params).await
}

pub async fn search_sequence(
    query: String,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> Result<types::Page<types::Sequence, Empty>, Box<dyn std::error::Error + Send + Sync>> {
    let mut params = get_allowed_langs_params(allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(format!("/api/v1/sequences/search/{query}").as_str(), params).await
}

pub async fn search_translator(
    query: String,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> Result<types::Page<types::Translator, Empty>, Box<dyn std::error::Error + Send + Sync>> {
    let mut params = get_allowed_langs_params(allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(
        format!("/api/v1/translators/search/{query}").as_str(),
        params,
    )
    .await
}

pub async fn get_book_annotation(
    id: u32,
) -> Result<types::BookAnnotation, Box<dyn std::error::Error + Send + Sync>> {
    _make_request(format!("/api/v1/books/{id}/annotation").as_str(), vec![]).await
}

pub async fn get_author_annotation(
    id: u32,
) -> Result<types::AuthorAnnotation, Box<dyn std::error::Error + Send + Sync>> {
    _make_request(format!("/api/v1/authors/{id}/annotation").as_str(), vec![]).await
}

pub async fn get_author_books(
    id: u32,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> Result<
    types::Page<types::AuthorBook, types::BookAuthor>,
    Box<dyn std::error::Error + Send + Sync>,
> {
    let mut params = get_allowed_langs_params(allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(format!("/api/v1/authors/{id}/books").as_str(), params).await
}

pub async fn get_translator_books(
    id: u32,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> Result<
    types::Page<types::TranslatorBook, types::BookTranslator>,
    Box<dyn std::error::Error + Send + Sync>,
> {
    let mut params = get_allowed_langs_params(allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(format!("/api/v1/translators/{id}/books").as_str(), params).await
}

pub async fn get_sequence_books(
    id: u32,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> Result<
    types::Page<types::SequenceBook, types::Sequence>,
    Box<dyn std::error::Error + Send + Sync>,
> {
    let mut params = get_allowed_langs_params(allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(format!("/api/v1/sequences/{id}/books").as_str(), params).await
}

pub async fn get_uploaded_books(
    page: u32,
    uploaded_gte: SmartString,
    uploaded_lte: SmartString,
) -> Result<types::Page<types::SearchBook, Empty>, Box<dyn std::error::Error + Send + Sync>> {
    let params = vec![
        ("page", page.to_string().into()),
        ("size", PAGE_SIZE.to_string().into()),
        ("uploaded_gte", uploaded_gte),
        ("uploaded_lte", uploaded_lte),
        ("is_deleted", "false".into()),
    ];

    _make_request("/api/v1/books", params).await
}

pub async fn get_author_books_available_types(
    id: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    let params = get_allowed_langs_params(allowed_langs);

    _make_request(
        format!("/api/v1/authors/{id}/available_types").as_str(),
        params,
    )
    .await
}

pub async fn get_translator_books_available_types(
    id: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    let params = get_allowed_langs_params(allowed_langs);

    _make_request(
        format!("/api/v1/translators/{id}/available_types").as_str(),
        params,
    )
    .await
}

pub async fn get_sequence_books_available_types(
    id: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    let params = get_allowed_langs_params(allowed_langs);

    _make_request(
        format!("/api/v1/sequences/{id}/available_types").as_str(),
        params,
    )
    .await
}
