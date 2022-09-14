use serde::Deserialize;

use super::formaters::Format;

#[derive(Deserialize, Debug, Clone)]
pub struct BookAuthor {
    id: u32,
    first_name: String,
    last_name: String,
    middle_name: String,
}

impl BookAuthor {
    pub fn format_author(&self) -> String {
        let BookAuthor {
            id,
            last_name,
            first_name,
            middle_name,
        } = self;

        format!("👤 {last_name} {first_name} {middle_name} /a_{id}")
    }

    pub fn format_translator(&self) -> String {
        let BookAuthor {
            id,
            first_name,
            last_name,
            middle_name,
        } = self;

        format!("👤 {last_name} {first_name} {middle_name} /t_{id}")
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct BookGenre {
    pub id: u32,
    pub description: String,
}

impl BookGenre {
    pub fn format(&self) -> String {
        format!("🗂 {}", self.description)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Source {
    // id: u32,
// name: String
}

#[derive(Deserialize, Debug, Clone)]
pub struct Book {
    pub id: u32,
    pub title: String,
    pub lang: String,
    // file_type: String,
    pub available_types: Vec<String>,
    // uploaded: String,
    pub annotation_exists: bool,
    pub authors: Vec<BookAuthor>,
    pub translators: Vec<BookAuthor>,
    pub sequences: Vec<Sequence>,
    pub genres: Vec<BookGenre>,
    // source: Source,
    // remote_id: u32,
    // id_deleted: bool,
    pub pages: Option<u32>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Author {
    pub id: u32,
    pub last_name: String,
    pub first_name: String,
    pub middle_name: String,
    pub annotation_exists: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Translator {
    pub id: u32,
    pub last_name: String,
    pub first_name: String,
    pub middle_name: String,
    pub annotation_exists: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Sequence {
    pub id: u32,
    pub name: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Genre {
    pub id: u32,
    pub source: Source,
    pub remote_id: u32,
    pub code: String,
    pub description: String,
    pub meta: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u32,
    pub page: u32,
    pub size: u32,
    pub total_pages: u32,
}

impl<T> Page<T>
where
    T: Format + Clone,
{
    pub fn format_items(&self) -> String {
        self.items
            .clone()
            .into_iter()
            .map(|book| book.format())
            .collect::<Vec<String>>()
            .join("\n\n\n")
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct SearchBook {
    pub id: u32,
    pub title: String,
    pub lang: String,
    // file_type: String,
    pub available_types: Vec<String>,
    // uploaded: String,
    pub annotation_exists: bool,
    pub authors: Vec<BookAuthor>,
    pub translators: Vec<BookAuthor>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BookAnnotation {
    pub id: u32,
    pub title: String,
    pub text: String,
    pub file: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AuthorAnnotation {
    pub id: u32,
    pub title: String,
    pub text: String,
    pub file: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AuthorBook {
    pub id: u32,
    pub title: String,
    pub lang: String,
    // file_type: String,
    pub available_types: Vec<String>,
    // uploaded: String,
    pub annotation_exists: bool,
    pub translators: Vec<BookAuthor>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TranslatorBook {
    pub id: u32,
    pub title: String,
    pub lang: String,
    // file_type: String,
    pub available_types: Vec<String>,
    // uploaded: String,
    pub annotation_exists: bool,
    pub authors: Vec<BookAuthor>,
}
