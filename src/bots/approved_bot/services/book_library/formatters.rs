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

fn format_authors(authors: Vec<BookAuthor>, count: usize) -> String {
    if count == 0 {
        return "".to_string();
    }

    match !authors.is_empty() {
        true => {
            let formated_authors = authors[..min(count, authors.len())]
                .iter()
                .map(|author| author.format_inline())
                .collect::<Vec<String>>()
                .join("\n");

            let post_fix = if authors.len() > count {
                "\nи др."
            } else {
                ""
            };
            format!("Авторы:\n{formated_authors}{post_fix}\n")
        }
        false => "".to_string(),
    }
}

fn format_translators(translators: Vec<BookTranslator>, count: usize) -> String {
    if count == 0 {
        return "".to_string();
    }

    match !translators.is_empty() {
        true => {
            let formated_translators = translators[..min(count, translators.len())]
                .iter()
                .map(|translator| translator.format_inline())
                .collect::<Vec<String>>()
                .join("\n");

            let post_fix = if translators.len() > count {
                "\nи др."
            } else {
                ""
            };
            format!("Переводчики:\n{formated_translators}{post_fix}\n")
        }
        false => "".to_string(),
    }
}

fn format_sequences(sequences: Vec<Sequence>, count: usize) -> String {
    if count == 0 {
        return "".to_string();
    }

    match !sequences.is_empty() {
        true => {
            let formated_sequences: String = sequences[..min(count, sequences.len())]
                .iter()
                .map(|sequence| sequence.format(NO_LIMIT).result)
                .collect::<Vec<String>>()
                .join("\n");

            let post_fix = if sequences.len() > count {
                "\nи др."
            } else {
                ""
            };
            format!("Серии:\n{formated_sequences}{post_fix}\n")
        }
        false => "".to_string(),
    }
}

fn format_genres(genres: Vec<BookGenre>, count: usize) -> String {
    if count == 0 {
        return "".to_string();
    }

    match !genres.is_empty() {
        true => {
            let formated_genres: String = genres[..min(count, genres.len())]
                .iter()
                .map(|genre| genre.format())
                .collect::<Vec<String>>()
                .join("\n");

            let post_fix = if genres.len() > count {
                "\nи др."
            } else {
                ""
            };
            format!("Жанры:\n{formated_genres}{post_fix}\n")
        }
        false => "".to_string(),
    }
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

    fn sub(self) -> Self {
        let Self {
            mut authors,
            mut translators,
            mut sequences,
            mut genres,
        } = self;

        if genres > 0 {
            genres -= 1;

            return Self {
                authors,
                translators,
                sequences,
                genres,
            };
        }

        if sequences > 0 {
            sequences -= 1;

            return Self {
                authors,
                translators,
                sequences,
                genres,
            };
        }

        if translators > 0 {
            translators -= 1;

            return Self {
                authors,
                translators,
                sequences,
                genres,
            };
        }

        if authors > 0 {
            authors -= 1;

            return Self {
                authors,
                translators,
                sequences,
                genres,
            };
        }

        Self {
            authors,
            translators,
            sequences,
            genres,
        }
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

impl Book {
    fn format_vectors(&self, max_size: usize) -> FormatVectorsResult {
        let mut counts = FormatVectorsCounts {
            authors: self.authors.len(),
            translators: self.translators.len(),
            sequences: self.sequences.len(),
            genres: self.genres.len(),
        };

        let mut result = FormatVectorsResult {
            authors: format_authors(self.authors.clone(), counts.authors),
            translators: format_translators(self.translators.clone(), counts.translators),
            sequences: format_sequences(self.sequences.clone(), counts.sequences),
            genres: format_genres(self.genres.clone(), counts.genres),
            max_result_size: 0,
        };

        let max_result_size = result.len();

        while result.len() > max_size && counts.can_sub() {
            counts = counts.sub();

            result = FormatVectorsResult {
                authors: format_authors(self.authors.clone(), counts.authors),
                translators: format_translators(self.translators.clone(), counts.translators),
                sequences: format_sequences(self.sequences.clone(), counts.sequences),
                genres: format_genres(self.genres.clone(), counts.genres),
                max_result_size: 0,
            };
        }

        result.with_max_result_size(max_result_size)
    }
}

impl Format for Book {
    fn format(&self, max_size: usize) -> FormatResult {
        let book_title = {
            let Book { title, lang, .. } = self;

            let year_part = match self.year {
                0 => "".to_string(),
                v => format!(" | {v}г."),
            };

            let pages_count = match self.pages {
                Some(1) | None => "".to_string(),
                Some(v) => format!(" | {v}с."),
            };

            let position_prefix = match self.position {
                Some(0) | None => "".to_string(),
                Some(v) => format!("{v} | "),
            };

            format!("{position_prefix}📖 {title} | {lang}{year_part}{pages_count}\n")
        };

        let annotations = match self.annotation_exists {
            true => {
                let Book { id, .. } = self;
                format!("📝 Аннотация: /b_an_{id}\n")
            }
            false => "".to_string(),
        };

        let download_command = (StartDownloadCommand { id: self.id }).to_string();
        let download_links = format!("Скачать:\n📥{download_command}");

        let required_data_len: usize = format!("{book_title}{annotations}{download_links}").len();
        let FormatVectorsResult {
            authors,
            translators,
            sequences,
            genres,
            max_result_size,
        } = self.format_vectors(max_size - required_data_len);

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
}

impl Format for SearchBook {
    fn format(&self, max_size: usize) -> FormatResult {
        Into::<Book>::into(self.clone()).format(max_size)
    }
}

impl Format for AuthorBook {
    fn format(&self, max_size: usize) -> FormatResult {
        Into::<Book>::into(self.clone()).format(max_size)
    }
}

impl Format for TranslatorBook {
    fn format(&self, max_size: usize) -> FormatResult {
        Into::<Book>::into(self.clone()).format(max_size)
    }
}

impl Format for SequenceBook {
    fn format(&self, max_size: usize) -> FormatResult {
        Into::<Book>::into(self.clone()).format(max_size)
    }
}
