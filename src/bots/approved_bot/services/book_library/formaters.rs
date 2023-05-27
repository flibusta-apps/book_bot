use std::cmp::min;

use crate::bots::approved_bot::modules::download::StartDownloadData;

use super::types::{
    AsBook, Author, AuthorBook, Book, BookAuthor, BookGenre, SearchBook, Sequence, Translator,
    TranslatorBook,
};

const NO_LIMIT: u32 = 4096;

pub trait Format {
    fn format(&self, max_size: u32) -> String;
}

pub trait FormatInline {
    fn format_inline(&self) -> String;
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

impl FormatInline for Translator {
    fn format_inline(&self) -> String {
        let Translator {
            id,
            first_name,
            last_name,
            middle_name,
            ..
        } = self;

        format!("👤 {last_name} {first_name} {middle_name} /t_{id}")
    }
}

fn format_authors(authors: Vec<BookAuthor>, count: usize) -> String {
    match !authors.is_empty() {
        true => {
            let formated_authors = authors.clone()[..min(count, authors.len())]
                .into_iter()
                .map(|author| author.format_inline())
                .collect::<Vec<String>>()
                .join("\n");

            let post_fix = if authors.len() > count { "\nи др." } else { "" };
            format!("Авторы:\n{formated_authors}{post_fix}\n")
        }
        false => "".to_string(),
    }
}

fn format_translators(translators: Vec<Translator>, count: usize) -> String {
    match !translators.is_empty() {
        true => {
            let formated_translators = translators.clone()[..min(count, translators.len())]
                .into_iter()
                .map(|translator| translator.format_inline())
                .collect::<Vec<String>>()
                .join("\n");

            let post_fix = if translators.len() > count { "\nи др." } else { "" };
            format!("Переводчики:\n{formated_translators}{post_fix}\n")
        }
        false => "".to_string(),
    }
}

fn format_sequences(sequences: Vec<Sequence>, count: usize) -> String {
    match !sequences.is_empty() {
        true => {
            let formated_sequences: String = sequences.clone()[..min(count, sequences.len())]
                .into_iter()
                .map(|sequence| sequence.format(NO_LIMIT))
                .collect::<Vec<String>>()
                .join("\n");

            let post_fix = if sequences.len() > count { "\nи др." } else { "" };
            format!("Серии:\n{formated_sequences}{post_fix}\n")
        }
        false => "".to_string(),
    }
}

fn format_genres(genres: Vec<BookGenre>, count: usize) -> String {
    match !genres.is_empty() {
        true => {
            let formated_genres: String = genres.clone()[..min(count, genres.len())]
                .into_iter()
                .map(|genre| genre.format())
                .collect::<Vec<String>>()
                .join("\n");

            let post_fix = if genres.len() > count { "\nи др." } else { "" };
            format!("Жанры:\n{formated_genres}{post_fix}\n")
        }
        false => "".to_string(),
    }
}

impl Format for Author {
    fn format(&self, _max_size: u32) -> String {
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

        format!("{title}{link}{annotation}")
    }
}

impl Format for Sequence {
    fn format(&self, _max_size: u32) -> String {
        let Sequence { id, name, .. } = self;

        let title = format!("📚 {name}");
        let link = format!("/s_{id}");

        format!("{title} {link}")
    }
}

impl Format for Translator {
    fn format(&self, _max_size: u32) -> String {
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

        format!("{title}{link}{annotation}")
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
        let Self {mut authors, mut translators, mut sequences, mut genres} = self;

        if translators > 0 {
            translators -= 1;

            return Self {
                authors,
                translators,
                sequences,
                genres
            }
        }

        if genres > 0 {
            genres -= 1;

            return Self {
                authors,
                translators,
                sequences,
                genres
            }
        }

        if sequences > 0 {
            sequences -= 1;

            return Self {
                authors,
                translators,
                sequences,
                genres
            }
        }

        if authors > 0 {
            authors -= 1;

            return Self {
                authors,
                translators,
                sequences,
                genres
            }
        }

        Self {
            authors,
            translators,
            sequences,
            genres
        }
    }
}

struct FormatVectorsResult {
    authors: String,
    translators: String,
    sequences: String,
    genres: String,
}

impl FormatVectorsResult {
    fn len(&self) -> usize {
        self.authors.len() + self.translators.len() + self.sequences.len() + self.genres.len()
    }
}

impl Book {
    fn format_vectors(&self, max_size: u32) -> FormatVectorsResult {
        let max_size_u: usize = max_size.try_into().unwrap();

        let mut counts = FormatVectorsCounts {
            authors: self.authors.len(),
            translators: self.translators.len(),
            sequences: self.sequences.len(),
            genres: self.genres.len()
        };

        let mut result = FormatVectorsResult {
            authors: format_authors(self.authors.clone(), counts.authors),
            translators: format_translators(self.translators.clone(), counts.translators),
            sequences: format_sequences(self.sequences.clone(), counts.sequences),
            genres: format_genres(self.genres.clone(), counts.genres),
        };

        while result.len() > max_size_u && counts.can_sub() {
            counts = counts.sub();

            result = FormatVectorsResult {
                authors: format_authors(self.authors.clone(), counts.authors),
                translators: format_translators(self.translators.clone(), counts.translators),
                sequences: format_sequences(self.sequences.clone(), counts.sequences),
                genres: format_genres(self.genres.clone(), counts.genres),
            };
        }

        result
    }
}

impl Format for Book {
    fn format(&self, max_size: u32) -> String {
        let book_title = {
            let Book { title, lang, .. } = self;

            let pages_count = match self.pages {
                Some(1) | None => "".to_string(),
                Some(v) => format!(" [ {v}с. ]\n\n"),
            };

            format!("📖 {title} | {lang}{pages_count}\n")
        };

        let annotations = match self.annotation_exists {
            true => {
                let Book { id, .. } = self;
                format!("📝 Аннотация: /b_an_{id}\n")
            }
            false => "".to_string(),
        };

        let download_command = (StartDownloadData { id: self.id }).to_string();
        let download_links = format!("Скачать:\n📥{download_command}");

        let required_data_len: u32 = format!("{book_title}{annotations}{download_links}").len().try_into().unwrap();
        let FormatVectorsResult { authors, translators, sequences, genres } = self.format_vectors(
            max_size - required_data_len
        );

        format!("{book_title}{annotations}{authors}{translators}{sequences}{genres}{download_links}")
    }
}

impl Format for SearchBook {
    fn format(&self, max_size: u32) -> String {
        self.clone().as_book().format(max_size)
    }
}

impl Format for AuthorBook {
    fn format(&self, max_size: u32) -> String {
        self.clone().as_book().format(max_size)
    }
}

impl Format for TranslatorBook {
    fn format(&self, max_size: u32) -> String {
        self.clone().as_book().format(max_size)
    }
}
