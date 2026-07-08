use core::fmt::Debug;
use serde::Deserialize;
use smallvec::SmallVec;

use super::formatters::{Format, FormatResult, FormatTitle};

#[derive(Default, Deserialize, Debug, Clone)]
pub struct BookAuthor {
    pub id: u32,
    pub first_name: String,
    pub last_name: String,
    pub middle_name: String,
}

#[derive(Default, Deserialize, Debug, Clone)]
pub struct BookTranslator {
    pub id: u32,
    pub first_name: String,
    pub last_name: String,
    pub middle_name: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BookGenre {
    // pub id: u32,
    pub description: String,
}

impl BookGenre {
    pub fn format(&self) -> String {
        format!("🗂 {}", self.description)
    }
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

#[derive(Default, Deserialize, Debug, Clone)]
pub struct Sequence {
    pub id: u32,
    pub name: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Genre {
    pub id: u32,
    // pub source: Source,
    // pub remote_id: u32,
    // pub code: String,
    pub description: String,
    // pub meta: String,
}

#[derive(Default, Deserialize, Debug, Clone)]
pub struct Empty {}

#[derive(Deserialize, Debug, Clone)]
pub struct Page<T, P> {
    pub items: Vec<T>,
    // pub total: u32,

    // pub page: u32,

    // pub size: u32,
    pub pages: u32,

    #[serde(default)]
    pub parent_item: Option<P>,
}

impl<T, P> Page<T, P>
where
    T: Format + Clone + Debug,
    P: FormatTitle + Clone + Debug,
{
    pub fn format(&self, page: u32, max_size: usize) -> String {
        let title: String = match &self.parent_item {
            Some(parent_item) => {
                let item_title = parent_item.format_title();

                if item_title.is_empty() {
                    return item_title;
                }

                format!("{item_title}\n\n\n")
            }
            None => "".to_string(),
        };

        let total_pages = self.pages;
        let footer = format!("\n\nСтраница {page}/{total_pages}");

        let formated_items = self.format_items(
            max_size
                .saturating_sub(title.len())
                .saturating_sub(footer.len()),
        );

        format!("{title}{formated_items}{footer}")
    }

    fn format_items(&self, max_size: usize) -> String {
        if self.items.is_empty() {
            return String::new();
        }

        let separator = "\n\n\n";
        let separator_len: usize = separator.len();

        let items_count: usize = self.items.len();
        let item_size: usize = max_size.saturating_sub(separator_len * items_count) / items_count;

        let format_result: Vec<FormatResult> = self
            .items
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
            .map(|item| item_size.saturating_sub(item.current_size))
            .sum();

        use std::borrow::Cow;

        self.items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let already_formated_result = &format_result[index];

                if already_formated_result.current_size == already_formated_result.max_size {
                    Cow::Borrowed(already_formated_result.result.as_str())
                } else {
                    let new_item_size = item_size + free_symbols;
                    let new_formated_result = item.format(new_item_size);

                    free_symbols = new_item_size.saturating_sub(new_formated_result.current_size);

                    Cow::Owned(new_formated_result.result)
                }
            })
            .collect::<Vec<Cow<str>>>()
            .join(separator)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct BookAnnotation {
    // pub id: u32,
    // pub title: String,
    pub text: String,
    pub file: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AuthorAnnotation {
    // pub id: u32,
    // pub title: String,
    pub text: String,
    pub file: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Book {
    pub id: u32,
    pub title: String,
    pub lang: String,
    pub available_types: SmallVec<[String; 4]>,
    pub annotation_exists: bool,
    pub authors: Vec<BookAuthor>,
    pub translators: Vec<BookTranslator>,
    pub sequences: Vec<Sequence>,
    pub genres: Vec<BookGenre>,
    pub year: i32,
    pub pages: Option<u32>,
    pub position: Option<i32>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SearchBook {
    pub id: u32,
    pub title: String,
    pub lang: String,
    pub annotation_exists: bool,
    pub authors: Vec<BookAuthor>,
    pub translators: Vec<BookTranslator>,
    pub sequences: Vec<Sequence>,
    pub year: i32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AuthorBook {
    pub id: u32,
    pub title: String,
    pub lang: String,
    pub annotation_exists: bool,
    pub translators: Vec<BookTranslator>,
    pub sequences: Vec<Sequence>,
    pub year: i32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TranslatorBook {
    pub id: u32,
    pub title: String,
    pub lang: String,
    pub annotation_exists: bool,
    pub authors: Vec<BookAuthor>,
    pub sequences: Vec<Sequence>,
    pub year: i32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SequenceBook {
    pub id: u32,
    pub title: String,
    pub lang: String,
    pub authors: Vec<BookAuthor>,
    pub translators: Vec<BookTranslator>,
    pub annotation_exists: bool,
    pub year: i32,
    pub position: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::approved_bot::services::book_library::formatters::FormatResult;

    // Minimal concrete types for testing Page::format_items
    #[derive(Clone, Debug)]
    struct FakeItem;

    impl crate::bots::approved_bot::services::book_library::formatters::Format for FakeItem {
        fn format(&self, max_size: usize) -> FormatResult {
            let s = "x".to_string();
            FormatResult {
                current_size: s.len(),
                max_size,
                result: s,
            }
        }
    }

    #[derive(Clone, Debug)]
    struct FakeParent;

    impl crate::bots::approved_bot::services::book_library::formatters::FormatTitle for FakeParent {
        fn format_title(&self) -> String {
            "parent".to_string()
        }
    }

    #[test]
    fn format_items_empty_does_not_panic() {
        let page: Page<FakeItem, FakeParent> = Page {
            items: vec![],
            pages: 1,
            parent_item: None,
        };
        let result = page.format_items(100);
        assert_eq!(result, "");
    }

    #[test]
    fn format_items_small_max_size_does_not_panic() {
        let page: Page<FakeItem, FakeParent> = Page {
            items: vec![FakeItem, FakeItem],
            pages: 1,
            parent_item: None,
        };
        // max_size smaller than the separators — previously could underflow
        let result = page.format_items(2);
        // should not panic; result may be truncated or empty
        let _ = result;
    }
}
