pub mod formatters;
pub mod types;

use smartstring::alias::String as SmartString;

use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use smallvec::SmallVec;

use crate::{
    bots::approved_bot::services::{build_url, check_response, HTTP_CLIENT},
    config,
};

use self::types::Empty;

fn get_allowed_langs_params(
    allowed_langs: &SmallVec<[SmartString; 3]>,
) -> Vec<(&'static str, SmartString)> {
    allowed_langs
        .into_iter()
        .map(|lang| ("allowed_langs", lang.clone()))
        .collect()
}

async fn _make_request<T>(
    segments: &[&str],
    params: Vec<(&str, SmartString)>,
) -> anyhow::Result<Option<T>>
where
    T: DeserializeOwned,
{
    let url = build_url(&config::CONFIG.book_server_url, segments.iter().copied())?;

    let response = HTTP_CLIENT
        .get(url)
        .query(&params)
        .header("Authorization", &config::CONFIG.book_server_api_key)
        .send()
        .await?;

    check_response(response, &[StatusCode::NOT_FOUND]).await
}

pub async fn get_book(id: u32) -> anyhow::Result<Option<types::Book>> {
    _make_request(&["api", "v1", "books", &id.to_string()], vec![]).await
}

pub async fn get_random_book_by_genre(
    allowed_langs: SmallVec<[SmartString; 3]>,
    genre: Option<u32>,
) -> anyhow::Result<Option<types::Book>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    if let Some(v) = genre {
        params.push(("genre", v.to_string().into()));
    }

    _make_request(&["api", "v1", "books", "random"], params).await
}

pub async fn get_random_book(
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Book>> {
    get_random_book_by_genre(allowed_langs, None).await
}

pub async fn get_random_author(
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Author>> {
    let params = get_allowed_langs_params(&allowed_langs);

    _make_request(&["api", "v1", "authors", "random"], params).await
}

pub async fn get_random_sequence(
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Sequence>> {
    let params = get_allowed_langs_params(&allowed_langs);

    _make_request(&["api", "v1", "sequences", "random"], params).await
}

pub async fn get_genre_metas() -> anyhow::Result<Option<Vec<String>>> {
    _make_request(&["api", "v1", "genres", "metas"], vec![]).await
}

pub async fn get_genres(
    meta: SmartString,
) -> anyhow::Result<Option<types::Page<types::Genre, Empty>>> {
    let params = vec![("meta", meta)];

    _make_request(&["api", "v1", "genres"], params).await
}

const PAGE_SIZE: &str = "5";

pub async fn search_book(
    query: String,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Page<types::SearchBook, Empty>>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(&["api", "v1", "books", "search", &query], params).await
}

pub async fn search_author(
    query: String,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Page<types::Author, Empty>>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(&["api", "v1", "authors", "search", &query], params).await
}

pub async fn search_sequence(
    query: String,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Page<types::Sequence, Empty>>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(&["api", "v1", "sequences", "search", &query], params).await
}

pub async fn search_translator(
    query: String,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Page<types::Translator, Empty>>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(&["api", "v1", "translators", "search", &query], params).await
}

pub async fn get_book_annotation(id: u32) -> anyhow::Result<Option<types::BookAnnotation>> {
    _make_request(
        &["api", "v1", "books", &id.to_string(), "annotation"],
        vec![],
    )
    .await
}

pub async fn get_author_annotation(id: u32) -> anyhow::Result<Option<types::AuthorAnnotation>> {
    _make_request(
        &["api", "v1", "authors", &id.to_string(), "annotation"],
        vec![],
    )
    .await
}

pub async fn get_author_books(
    id: u32,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Page<types::AuthorBook, types::Person>>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(&["api", "v1", "authors", &id.to_string(), "books"], params).await
}

pub async fn get_translator_books(
    id: u32,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Page<types::TranslatorBook, types::Person>>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    let mut result: Option<types::Page<types::TranslatorBook, types::Person>> = _make_request(
        &["api", "v1", "translators", &id.to_string(), "books"],
        params,
    )
    .await?;

    if let Some(page) = result.as_mut() {
        if let Some(parent) = page.parent_item.as_mut() {
            parent.kind = types::PersonKind::Translator;
        }
    }

    Ok(result)
}

pub async fn get_sequence_books(
    id: u32,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Page<types::SequenceBook, types::Sequence>>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(
        &["api", "v1", "sequences", &id.to_string(), "books"],
        params,
    )
    .await
}

pub async fn get_uploaded_books(
    page: u32,
    uploaded_gte: SmartString,
    uploaded_lte: SmartString,
) -> anyhow::Result<Option<types::Page<types::SearchBook, Empty>>> {
    let params = vec![
        ("page", page.to_string().into()),
        ("size", PAGE_SIZE.to_string().into()),
        ("uploaded_gte", uploaded_gte),
        ("uploaded_lte", uploaded_lte),
        ("is_deleted", "false".into()),
    ];

    _make_request(&["api", "v1", "books"], params).await
}

pub async fn get_author_books_available_types(
    id: u32,
    allowed_langs: &SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<Vec<String>>> {
    let params = get_allowed_langs_params(allowed_langs);

    _make_request(
        &["api", "v1", "authors", &id.to_string(), "available_types"],
        params,
    )
    .await
}

pub async fn get_translator_books_available_types(
    id: u32,
    allowed_langs: &SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<Vec<String>>> {
    let params = get_allowed_langs_params(allowed_langs);

    _make_request(
        &[
            "api",
            "v1",
            "translators",
            &id.to_string(),
            "available_types",
        ],
        params,
    )
    .await
}

pub async fn get_sequence_books_available_types(
    id: u32,
    allowed_langs: &SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<Vec<String>>> {
    let params = get_allowed_langs_params(allowed_langs);

    _make_request(
        &["api", "v1", "sequences", &id.to_string(), "available_types"],
        params,
    )
    .await
}
