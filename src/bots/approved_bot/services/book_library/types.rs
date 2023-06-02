use serde::Deserialize;

use super::formaters::{Format, FormatResult};


#[derive(Deserialize, Debug, Clone)]
pub struct BookAuthor {
    pub id: u32,
    pub first_name: String,
    pub last_name: String,
    pub middle_name: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BookTranslator {
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
    pub fn format_items(&self, max_size: usize) -> String {
        let separator = "\n\n\n";
        let separator_len: usize = separator.len();

        let items_count: usize = self.items.len();
        let item_size: usize = (max_size - separator_len * items_count) / items_count;

        let format_result: Vec<FormatResult> = self.items
            .iter()
            .map(|item| item.format(item_size))
            .collect();

        let has_any_spliced = {
            format_result
                .iter()
                .any(|item| item.current_size != item.max_size)
        };

        if !has_any_spliced {
            return format_result
                .into_iter()
                .map(|item| item.result)
                .collect::<Vec<String>>()
                .join(separator);
        }

        let mut free_symbols: usize = format_result
            .iter()
            .filter(|item| item.current_size == item.max_size)
            .map(|item| item_size - item.current_size)
            .sum();

        self.items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let already_formated_result = &format_result[index];

                if already_formated_result.current_size == already_formated_result.max_size {
                    already_formated_result.result.clone()
                } else {
                    let new_item_size = item_size + free_symbols;
                    let new_formated_result = item.format(new_item_size);

                    free_symbols = new_item_size - new_formated_result.current_size;

                    new_formated_result.result
                }
            })
            .collect::<Vec<String>>()
            .join(separator)
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
    pub translators: Vec<BookTranslator>,
    pub sequences: Vec<Sequence>,
    pub genres: Vec<BookGenre>,
    // source: Source,
    // remote_id: u32,
    // id_deleted: bool,
    pub pages: Option<u32>,
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
    pub translators: Vec<BookTranslator>,
    pub sequences: Vec<Sequence>,
}

impl From<SearchBook> for Book {
    fn from(value: SearchBook) -> Self {
        Book {
            id: value.id,
            title: value.title,
            lang: value.lang,
            available_types: value.available_types,
            annotation_exists: value.annotation_exists,
            authors: value.authors,
            translators: value.translators,
            sequences: value.sequences,
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
    pub translators: Vec<BookTranslator>,
    pub sequences: Vec<Sequence>,
}

impl From<AuthorBook> for Book {
    fn from(value: AuthorBook) -> Self {
        Book {
            id: value.id,
            title: value.title,
            lang: value.lang,
            available_types: value.available_types,
            annotation_exists: value.annotation_exists,
            authors: vec![],
            translators: value.translators,
            sequences: value.sequences,
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

impl From<TranslatorBook> for Book {
    fn from(value: TranslatorBook) -> Self {
        Book {
            id: value.id,
            title: value.title,
            lang: value.lang,
            available_types: value.available_types,
            annotation_exists: value.annotation_exists,
            authors: value.authors,
            translators: vec![],
            sequences: value.sequences,
            genres: vec![],
            pages: None
        }
    }
}


#[derive(Deserialize, Debug, Clone)]
pub struct SequenceBook {
    pub id: u32,
    pub title: String,
    pub lang: String,
    // file_type: String,
    pub available_types: Vec<String>,
    // uploaded: String,
    pub authors: Vec<BookAuthor>,
    pub translators: Vec<BookTranslator>,
    pub annotation_exists: bool,
}

impl From<SequenceBook> for Book {
    fn from(value: SequenceBook) -> Self {
        Book {
            id: value.id,
            title: value.title,
            lang: value.lang,
            available_types: value.available_types,
            annotation_exists: value.annotation_exists,
            authors: value.authors,
            translators: value.translators,
            sequences: vec![],
            genres: vec![],
            pages: None
        }
    }
}
