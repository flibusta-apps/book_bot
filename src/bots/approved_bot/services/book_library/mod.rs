pub mod formaters;
pub mod types;

use serde::de::DeserializeOwned;

use crate::config;

fn get_allowed_langs_params(allowed_langs: Vec<String>) -> Vec<(&'static str, String)> {
    allowed_langs
        .into_iter()
        .map(|lang| ("allowed_langs", lang))
        .collect()
}

async fn _make_request<T>(
    url: &str,
    params: Vec<(&str, String)>,
) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
where
    T: DeserializeOwned,
{
    let response = reqwest::Client::new()
        .get(format!("{}{}", &config::CONFIG.book_server_url, url))
        .query(&params)
        .header("Authorization", &config::CONFIG.book_server_api_key)
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<T>().await?)
}

pub async fn get_random_book_by_genre(
    allowed_langs: Vec<String>,
    genre: Option<u32>,
) -> Result<types::Book, Box<dyn std::error::Error + Send + Sync>> {
    let mut params: Vec<(&str, String)> = get_allowed_langs_params(allowed_langs);

    if let Some(v) = genre {
        params.push(("genre", v.to_string()));
    }

    _make_request("/api/v1/books/random", params).await
}

pub async fn get_random_book(
    allowed_langs: Vec<String>,
) -> Result<types::Book, Box<dyn std::error::Error + Send + Sync>> {
    get_random_book_by_genre(allowed_langs, None).await
}

pub async fn get_random_author(
    allowed_langs: Vec<String>,
) -> Result<types::Author, Box<dyn std::error::Error + Send + Sync>> {
    let params: Vec<(&str, String)> = get_allowed_langs_params(allowed_langs);

    _make_request("/api/v1/authors/random", params).await
}

pub async fn get_random_sequence(
    allowed_langs: Vec<String>,
) -> Result<types::Sequence, Box<dyn std::error::Error + Send + Sync>> {
    let params = get_allowed_langs_params(allowed_langs);

    _make_request("/api/v1/sequences/random", params).await
}

pub async fn get_genre_metas() -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    _make_request("/api/v1/genres/metas", vec![]).await
}

pub async fn get_genres(
    meta: String,
) -> Result<types::Page<types::Genre>, Box<dyn std::error::Error + Send + Sync>> {
    let params = vec![("meta", meta)];

    _make_request("/api/v1/genres/", params).await
}

const PAGE_SIZE: &str = "7";

pub async fn search_book(
    query: String,
    page: u32,
    allowed_langs: Vec<String>,
) -> Result<types::Page<types::SearchBook>, Box<dyn std::error::Error + Send + Sync>> {
    let mut params = get_allowed_langs_params(allowed_langs);

    params.push(("page", page.to_string()));
    params.push(("size", PAGE_SIZE.to_string()));

    _make_request(format!("/api/v1/books/search/{query}").as_str(), params).await
}

pub async fn search_author(
    query: String,
    page: u32,
    allowed_langs: Vec<String>,
) -> Result<types::Page<types::Author>, Box<dyn std::error::Error + Send + Sync>> {
    let mut params = get_allowed_langs_params(allowed_langs);

    params.push(("page", page.to_string()));
    params.push(("size", PAGE_SIZE.to_string()));

    _make_request(format!("/api/v1/authors/search/{query}").as_str(), params).await
}

pub async fn search_sequence(
    query: String,
    page: u32,
    allowed_langs: Vec<String>,
) -> Result<types::Page<types::Sequence>, Box<dyn std::error::Error + Send + Sync>> {
    let mut params = get_allowed_langs_params(allowed_langs);

    params.push(("page", page.to_string()));
    params.push(("size", PAGE_SIZE.to_string()));

    _make_request(format!("/api/v1/sequences/search/{query}").as_str(), params).await
}

pub async fn search_translator(
    query: String,
    page: u32,
    allowed_langs: Vec<String>,
) -> Result<types::Page<types::Translator>, Box<dyn std::error::Error + Send + Sync>> {
    let mut params = get_allowed_langs_params(allowed_langs);

    params.push(("page", page.to_string()));
    params.push(("size", PAGE_SIZE.to_string()));

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
    allowed_langs: Vec<String>,
) -> Result<types::Page<types::AuthorBook>, Box<dyn std::error::Error + Send + Sync>> {
    let mut params = get_allowed_langs_params(allowed_langs);

    params.push(("page", page.to_string()));
    params.push(("size", PAGE_SIZE.to_string()));

    _make_request(format!("/api/v1/authors/{id}/books").as_str(), params).await
}

pub async fn get_translator_books(
    id: u32,
    page: u32,
    allowed_langs: Vec<String>,
) -> Result<types::Page<types::TranslatorBook>, Box<dyn std::error::Error + Send + Sync>> {
    let mut params = get_allowed_langs_params(allowed_langs);

    params.push(("page", page.to_string()));
    params.push(("size", PAGE_SIZE.to_string()));

    _make_request(format!("/api/v1/translators/{id}/books").as_str(), params).await
}

pub async fn get_sequence_books(
    id: u32,
    page: u32,
    allowed_langs: Vec<String>,
) -> Result<types::Page<types::SearchBook>, Box<dyn std::error::Error + Send + Sync>> {
    let mut params = get_allowed_langs_params(allowed_langs);

    params.push(("page", page.to_string()));
    params.push(("size", PAGE_SIZE.to_string()));

    _make_request(format!("/api/v1/sequences/{id}/books").as_str(), params).await
}

pub async fn get_uploaded_books(
    page: u32,
    uploaded_gte: String,
    uploaded_lte: String,
) -> Result<types::Page<types::SearchBook>, Box<dyn std::error::Error + Send + Sync>> {
    let params = vec![
        ("page", page.to_string()),
        ("size", PAGE_SIZE.to_string()),
        ("uploaded_gte", uploaded_gte),
        ("uploaded_lte", uploaded_lte),
        ("is_deleted", "false".to_string()),
    ];

    _make_request("/api/v1/books/", params).await
}
