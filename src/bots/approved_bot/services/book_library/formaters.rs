use std::cmp::min;

use crate::bots::approved_bot::modules::download::StartDownloadData;

use super::types::{Author, AuthorBook, Book, SearchBook, Sequence, Translator, TranslatorBook, BookAuthor, BookGenre, AsBook};

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
            let formated_authors = authors
                .clone()[..min(count, authors.len())]
                .into_iter()
                .map(|author| author.format_inline())
                .collect::<Vec<String>>()
                .join("\n");
            format!("Авторы:\n{formated_authors}\n\n")
        }
        false => "".to_string(),
    }
}


fn format_translators(translators: Vec<Translator>, count: usize) -> String {
    match !translators.is_empty() {
        true => {
            let formated_translators = translators
                .clone()[..min(count, translators.len())]
                .into_iter()
                .map(|translator| translator.format_inline())
                .collect::<Vec<String>>()
                .join("\n");
            format!("Переводчики:\n{formated_translators}\n\n")
        }
        false => "".to_string(),
    }
}


fn format_sequences(sequences: Vec<Sequence>, count: usize) -> String {
    match !sequences.is_empty() {
        true => {
            let formated_sequences: String = sequences
                .clone()[..min(count, sequences.len())]
                .into_iter()
                .map(|sequence| sequence.format(NO_LIMIT))
                .collect::<Vec<String>>()
                .join("\n");
            format!("Серии:\n{formated_sequences}\n\n")
        }
        false => "".to_string(),
    }
}


fn format_genres(genres: Vec<BookGenre>, count: usize) -> String {
    match !genres.is_empty() {
        true => {
            let formated_genres: String = genres
                .clone()[..min(count, genres.len())]
                .into_iter()
                .map(|genre| genre.format())
                .collect::<Vec<String>>()
                .join("\n");
            format!("Жанры:\n{formated_genres}\n\n")
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


impl Format for Book {
    fn format(&self, max_size: u32) -> String {
        let book_title = {
            let Book { title, lang, .. } = self;
            format!("📖 {title} | {lang}\n")
        };

        let pages_count = match self.pages {
            Some(1) | None => "".to_string(),
            Some(v) => format!("[ {v}с. ]\n\n"),
        };

        let annotations = match self.annotation_exists {
            true => {
                let Book { id, .. } = self;
                format!("📝 Аннотация: /b_an_{id}\n\n")
            }
            false => "".to_string(),
        };

        let download_command = (StartDownloadData { id: self.id }).to_string();
        let download_links = format!("Скачать:\n📥{download_command}");

        let authors = format_authors(self.authors.clone(), self.authors.len());
        let translators = format_translators(self.translators.clone(), self.translators.len());
        let sequences = format_sequences(self.sequences.clone(), self.sequences.len());
        let genres = format_genres(self.genres.clone(), self.genres.len());

        format!("{book_title}{pages_count}{annotations}{authors}{translators}{sequences}{genres}{download_links}")
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
