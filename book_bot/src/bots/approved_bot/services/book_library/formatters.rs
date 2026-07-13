use std::cmp::min;

use crate::bots::approved_bot::modules::download::commands::{
    DownloadArchiveCommand, StartDownloadCommand,
};

use super::types::{
    Author, AuthorBook, Book, BookAuthor, BookGenre, BookTranslator, Empty, SearchBook, Sequence,
    SequenceBook, Translator, TranslatorBook,
};

const NO_LIMIT: usize = 4096;

#[derive(Clone)]
pub struct FormatResult {
    pub result: String,

    pub current_size: usize,
    pub max_size: usize,
}

pub trait Format {
    fn format(&self, max_size: usize) -> FormatResult;
}

pub trait FormatInline {
    fn format_inline(&self) -> String;
}

pub trait FormatTitle {
    fn format_title(&self) -> String;
}

impl FormatTitle for Empty {
    fn format_title(&self) -> String {
        "".to_string()
    }
}

impl FormatTitle for BookAuthor {
    fn format_title(&self) -> String {
        let BookAuthor {
            id,
            last_name,
            first_name,
            middle_name,
        } = self;

        if *id == 0 {
            return "".to_string();
        }

        let command = (DownloadArchiveCommand::Author { id: *id }).to_string();

        format!("👤 {last_name} {first_name} {middle_name}\nСкачать все книги архивом: {command}")
    }
}

impl FormatTitle for BookTranslator {
    fn format_title(&self) -> String {
        let BookTranslator {
            id,
            first_name,
            last_name,
            middle_name,
        } = self;

        if *id == 0 {
            return "".to_string();
        }

        let command = (DownloadArchiveCommand::Translator { id: *id }).to_string();

        format!("👤 {last_name} {first_name} {middle_name}\nСкачать все книги архивом: {command}")
    }
}

impl FormatTitle for Sequence {
    fn format_title(&self) -> String {
        let Sequence { id, name } = self;

        if *id == 0 {
            return "".to_string();
        }

        let command = (DownloadArchiveCommand::Sequence { id: *id }).to_string();

        format!("📚 {name}\nСкачать все книги архивом: {command}")
    }
}

impl FormatInline for BookAuthor {
    fn format_inline(&self) -> String {
        let BookAuthor {
            id,
            last_name,
            first_name,
            middle_name,
        } = self;

        format!("👤 {last_name} {first_name} {middle_name} /a_{id}")
    }
}

impl FormatInline for BookTranslator {
    fn format_inline(&self) -> String {
        let BookTranslator {
            id,
            first_name,
            last_name,
            middle_name,
        } = self;

        format!("👤 {last_name} {first_name} {middle_name} /t_{id}")
    }
}

fn format_list<T>(items: &[T], count: usize, header: &str, fmt: impl Fn(&T) -> String) -> String {
    if count == 0 || items.is_empty() {
        return "".to_string();
    }

    let formatted_items = items[..min(count, items.len())]
        .iter()
        .map(fmt)
        .collect::<Vec<String>>()
        .join("\n");

    let post_fix = if items.len() > count {
        "\nи др."
    } else {
        ""
    };

    format!("{header}{formatted_items}{post_fix}\n")
}

impl Format for Author {
    fn format(&self, _max_size: usize) -> FormatResult {
        let Author {
            id,
            last_name,
            first_name,
            middle_name,
            ..
        } = self;

        let title = format!("👤 {last_name} {first_name} {middle_name}\n");
        let link = format!("/a_{id}\n");
        let annotation = match self.annotation_exists {
            true => format!("📝 Аннотация: /a_an_{id}"),
            false => "".to_string(),
        };

        let result = format!("{title}{link}{annotation}");
        let result_len = result.len();

        FormatResult {
            result,
            current_size: result_len,
            max_size: result_len,
        }
    }
}

impl Format for Sequence {
    fn format(&self, _max_size: usize) -> FormatResult {
        let Sequence { id, name, .. } = self;

        let title = format!("📚 {name}");
        let link = format!("/s_{id}");

        let result = format!("{title} {link}");
        let result_len = result.len();

        FormatResult {
            result,
            current_size: result_len,
            max_size: result_len,
        }
    }
}

impl Format for Translator {
    fn format(&self, _max_size: usize) -> FormatResult {
        let Translator {
            id,
            last_name,
            first_name,
            middle_name,
            ..
        } = self;

        let title = format!("👤 {last_name} {first_name} {middle_name}\n");
        let link = format!("/t_{id}\n");
        let annotation = match self.annotation_exists {
            true => format!("📝 Аннотация: /a_an_{id}"),
            false => "".to_string(),
        };

        let result = format!("{title}{link}{annotation}");
        let result_len = result.len();

        FormatResult {
            result,
            current_size: result_len,
            max_size: result_len,
        }
    }
}

struct FormatVectorsCounts {
    authors: usize,
    translators: usize,
    sequences: usize,
    genres: usize,
}

impl FormatVectorsCounts {
    fn sum(&self) -> usize {
        self.authors + self.translators + self.sequences + self.genres
    }

    fn can_sub(&self) -> bool {
        self.sum() > 0
    }

    fn sub(mut self) -> Self {
        for count in [
            &mut self.genres,
            &mut self.sequences,
            &mut self.translators,
            &mut self.authors,
        ] {
            if *count > 0 {
                *count -= 1;
                break;
            }
        }

        self
    }
}

struct FormatVectorsResult {
    authors: String,
    translators: String,
    sequences: String,
    genres: String,

    max_result_size: usize,
}

impl FormatVectorsResult {
    fn len(&self) -> usize {
        self.authors.len() + self.translators.len() + self.sequences.len() + self.genres.len()
    }

    fn with_max_result_size(self, max_result_size: usize) -> Self {
        let Self {
            authors,
            translators,
            sequences,
            genres,
            ..
        } = self;

        Self {
            authors,
            translators,
            sequences,
            genres,
            max_result_size,
        }
    }
}

fn format_vectors(
    authors: &[BookAuthor],
    translators: &[BookTranslator],
    sequences: &[Sequence],
    genres: &[BookGenre],
    max_size: usize,
) -> FormatVectorsResult {
    let mut counts = FormatVectorsCounts {
        authors: authors.len(),
        translators: translators.len(),
        sequences: sequences.len(),
        genres: genres.len(),
    };

    let mut result = FormatVectorsResult {
        authors: format_list(authors, counts.authors, "Авторы:\n", |a| {
            a.format_inline()
        }),
        translators: format_list(
            translators,
            counts.translators,
            "Переводчики:\n",
            |t| t.format_inline(),
        ),
        sequences: format_list(sequences, counts.sequences, "Серии:\n", |s| {
            s.format(NO_LIMIT).result
        }),
        genres: format_list(genres, counts.genres, "Жанры:\n", |g| g.format()),
        max_result_size: 0,
    };

    let max_result_size = result.len();

    while result.len() > max_size && counts.can_sub() {
        counts = counts.sub();

        result = FormatVectorsResult {
            authors: format_list(authors, counts.authors, "Авторы:\n", |a| {
                a.format_inline()
            }),
            translators: format_list(
                translators,
                counts.translators,
                "Переводчики:\n",
                |t| t.format_inline(),
            ),
            sequences: format_list(sequences, counts.sequences, "Серии:\n", |s| {
                s.format(NO_LIMIT).result
            }),
            genres: format_list(genres, counts.genres, "Жанры:\n", |g| g.format()),
            max_result_size: 0,
        };
    }

    result.with_max_result_size(max_result_size)
}

struct FormatData<'a> {
    pub id: u32,
    pub title: &'a str,
    pub lang: &'a str,
    pub annotation_exists: bool,
    pub authors: &'a [BookAuthor],
    pub translators: &'a [BookTranslator],
    pub sequences: &'a [Sequence],
    pub genres: &'a [BookGenre],
    pub year: i32,
    pub pages: Option<u32>,
    pub position: Option<i32>,
}

fn format_common(data: FormatData, max_size: usize) -> FormatResult {
    let FormatData {
        id,
        title,
        lang,
        annotation_exists,
        authors,
        translators,
        sequences,
        genres,
        year,
        pages,
        position,
    } = data;

    let book_title = {
        let year_part = match year {
            0 => "".to_string(),
            v => format!(" | {v}г."),
        };

        let pages_count = match pages {
            Some(1) | None => "".to_string(),
            Some(v) => format!(" | {v}с."),
        };

        let position_prefix = match position {
            Some(0) | None => "".to_string(),
            Some(v) => format!("{v} | "),
        };

        format!("{position_prefix}📖 {title} | {lang}{year_part}{pages_count}\n")
    };

    let annotations = match annotation_exists {
        true => {
            format!("📝 Аннотация: /b_an_{id}\n")
        }
        false => "".to_string(),
    };

    let download_command = (StartDownloadCommand { id }).to_string();
    let download_links = format!("Скачать:\n📥{download_command}");

    let required_data_len: usize = format!("{book_title}{annotations}{download_links}").len();
    let FormatVectorsResult {
        authors,
        translators,
        sequences,
        genres,
        max_result_size,
    } = format_vectors(
        authors,
        translators,
        sequences,
        genres,
        max_size.saturating_sub(required_data_len),
    );

    let result = format!(
        "{book_title}{annotations}{authors}{translators}{sequences}{genres}{download_links}"
    );
    let result_len = result.len();

    FormatResult {
        result,
        current_size: result_len,
        max_size: max_result_size + required_data_len,
    }
}

impl Format for Book {
    fn format(&self, max_size: usize) -> FormatResult {
        format_common(
            FormatData {
                id: self.id,
                title: &self.title,
                lang: &self.lang,
                annotation_exists: self.annotation_exists,
                authors: &self.authors,
                translators: &self.translators,
                sequences: &self.sequences,
                genres: &self.genres,
                year: self.year,
                pages: self.pages,
                position: self.position,
            },
            max_size,
        )
    }
}

impl Format for SearchBook {
    fn format(&self, max_size: usize) -> FormatResult {
        format_common(
            FormatData {
                id: self.id,
                title: &self.title,
                lang: &self.lang,
                annotation_exists: self.annotation_exists,
                authors: &self.authors,
                translators: &self.translators,
                sequences: &self.sequences,
                genres: &[],
                year: self.year,
                pages: None,
                position: None,
            },
            max_size,
        )
    }
}

impl Format for AuthorBook {
    fn format(&self, max_size: usize) -> FormatResult {
        format_common(
            FormatData {
                id: self.id,
                title: &self.title,
                lang: &self.lang,
                annotation_exists: self.annotation_exists,
                authors: &[],
                translators: &self.translators,
                sequences: &self.sequences,
                genres: &[],
                year: self.year,
                pages: None,
                position: None,
            },
            max_size,
        )
    }
}

impl Format for TranslatorBook {
    fn format(&self, max_size: usize) -> FormatResult {
        format_common(
            FormatData {
                id: self.id,
                title: &self.title,
                lang: &self.lang,
                annotation_exists: self.annotation_exists,
                authors: &self.authors,
                translators: &[],
                sequences: &self.sequences,
                genres: &[],
                year: self.year,
                pages: None,
                position: None,
            },
            max_size,
        )
    }
}

impl Format for SequenceBook {
    fn format(&self, max_size: usize) -> FormatResult {
        format_common(
            FormatData {
                id: self.id,
                title: &self.title,
                lang: &self.lang,
                annotation_exists: self.annotation_exists,
                authors: &self.authors,
                translators: &self.translators,
                sequences: &[],
                genres: &[],
                year: self.year,
                pages: None,
                position: Some(self.position),
            },
            max_size,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{format_list, FormatVectorsCounts};

    #[test]
    fn count_zero_yields_empty_string() {
        let items = vec!["a".to_string(), "b".to_string()];
        assert_eq!(format_list(&items, 0, "Header:\n", |s| s.clone()), "");
    }

    #[test]
    fn empty_items_yields_empty_string_even_with_positive_count() {
        let items: Vec<String> = vec![];
        assert_eq!(format_list(&items, 5, "Header:\n", |s| s.clone()), "");
    }

    #[test]
    fn formats_up_to_count_items_with_header_and_no_suffix_when_exact() {
        let items = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            format_list(&items, 2, "Header:\n", |s| s.clone()),
            "Header:\na\nb\n"
        );
    }

    #[test]
    fn truncates_to_count_and_appends_i_dr_suffix_when_more_items_exist() {
        let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(
            format_list(&items, 2, "Header:\n", |s| s.clone()),
            "Header:\na\nb\nи др.\n"
        );
    }

    #[test]
    fn sub_decrements_genres_first() {
        let counts = FormatVectorsCounts {
            authors: 1,
            translators: 1,
            sequences: 1,
            genres: 1,
        }
        .sub();
        assert_eq!(
            (
                counts.authors,
                counts.translators,
                counts.sequences,
                counts.genres
            ),
            (1, 1, 1, 0)
        );
    }

    #[test]
    fn sub_decrements_sequences_when_genres_already_zero() {
        let counts = FormatVectorsCounts {
            authors: 1,
            translators: 1,
            sequences: 1,
            genres: 0,
        }
        .sub();
        assert_eq!(
            (
                counts.authors,
                counts.translators,
                counts.sequences,
                counts.genres
            ),
            (1, 1, 0, 0)
        );
    }

    #[test]
    fn sub_decrements_translators_when_genres_and_sequences_zero() {
        let counts = FormatVectorsCounts {
            authors: 1,
            translators: 1,
            sequences: 0,
            genres: 0,
        }
        .sub();
        assert_eq!(
            (
                counts.authors,
                counts.translators,
                counts.sequences,
                counts.genres
            ),
            (1, 0, 0, 0)
        );
    }

    #[test]
    fn sub_decrements_authors_last() {
        let counts = FormatVectorsCounts {
            authors: 1,
            translators: 0,
            sequences: 0,
            genres: 0,
        }
        .sub();
        assert_eq!(
            (
                counts.authors,
                counts.translators,
                counts.sequences,
                counts.genres
            ),
            (0, 0, 0, 0)
        );
    }

    #[test]
    fn sub_is_a_no_op_when_all_already_zero() {
        let counts = FormatVectorsCounts {
            authors: 0,
            translators: 0,
            sequences: 0,
            genres: 0,
        }
        .sub();
        assert_eq!(
            (
                counts.authors,
                counts.translators,
                counts.sequences,
                counts.genres
            ),
            (0, 0, 0, 0)
        );
    }
}
