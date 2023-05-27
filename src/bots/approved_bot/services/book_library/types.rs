use serde::Deserialize;

use super::formaters::Format;


#[derive(Deserialize, Debug, Clone)]
pub struct BookAuthor {
    pub id: u32,
    pub first_name: String,
    pub last_name: String,
    pub middle_name: String,
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
    pub fn format_items(&self, max_size: u32) -> String {
        let items_count: u32 = self.items.len().try_into().unwrap();
        let item_size: u32 = (max_size - 3 * items_count) / items_count;

        self.items
            .clone()
            .into_iter()
            .map(|item| item.format(item_size))
            .collect::<Vec<String>>()
            .join("\n\n\n")
    }
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

pub trait AsBook<T> {
    fn as_book(self) -> T;
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
    pub translators: Vec<Translator>,
    pub sequences: Vec<Sequence>,
    pub genres: Vec<BookGenre>,
    // source: Source,
    // remote_id: u32,
    // id_deleted: bool,
    pub pages: Option<u32>,
}

impl AsBook<Book> for Book {
    fn as_book(self) -> Book {
        self
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
    pub translators: Vec<Translator>,
    pub sequences: Vec<Sequence>,
}

impl AsBook<Book> for SearchBook {
    fn as_book(self) -> Book {
        Book {
            id: self.id,
            title: self.title,
            lang: self.lang,
            available_types: self.available_types,
            annotation_exists: self.annotation_exists,
            authors: self.authors,
            translators: self.translators,
            sequences: self.sequences,
            genres: vec![],
            pages: None
        }
    }
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
    pub translators: Vec<Translator>,
    pub sequences: Vec<Sequence>,
}

impl AsBook<Book> for AuthorBook {
    fn as_book(self) -> Book {
        Book {
            id: self.id,
            title: self.title,
            lang: self.lang,
            available_types: self.available_types,
            annotation_exists: self.annotation_exists,
            authors: vec![],
            translators: self.translators,
            sequences: self.sequences,
            genres: vec![],
            pages: None
        }
    }
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
    pub sequences: Vec<Sequence>,
}

impl AsBook<Book> for TranslatorBook {
    fn as_book(self) -> Book {
        Book {
            id: self.id,
            title: self.title,
            lang: self.lang,
            available_types: self.available_types,
            annotation_exists: self.annotation_exists,
            authors: self.authors,
            translators: vec![],
            sequences: self.sequences,
            genres: vec![],
            pages: None
        }
    }
}
