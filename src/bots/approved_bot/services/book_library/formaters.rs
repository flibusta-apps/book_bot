use super::types::{Author, AuthorBook, Book, SearchBook, Sequence, Translator, TranslatorBook};

pub trait Format {
    fn format(&self) -> String;
}

impl Format for Book {
    fn format(&self) -> String {
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

        let authors = match self.authors.len() != 0 {
            true => {
                let formated_authors = self
                    .authors
                    .clone()
                    .into_iter()
                    .map(|author| author.format_author())
                    .collect::<Vec<String>>()
                    .join("\n");
                format!("Авторы:\n{formated_authors}\n\n")
            }
            false => "".to_string(),
        };

        let translators = match self.translators.len() != 0 {
            true => {
                let formated_translators = self
                    .translators
                    .clone()
                    .into_iter()
                    .map(|translator| translator.format_translator())
                    .collect::<Vec<String>>()
                    .join("\n");
                format!("Переводчики:\n{formated_translators}\n\n")
            }
            false => "".to_string(),
        };

        let sequences = match self.sequences.len() != 0 {
            true => {
                let formated_sequences: String = self
                    .sequences
                    .clone()
                    .into_iter()
                    .map(|sequence| sequence.format())
                    .collect::<Vec<String>>()
                    .join("\n");
                format!("Серии:\n{formated_sequences}\n\n")
            }
            false => "".to_string(),
        };

        let genres = match self.genres.len() != 0 {
            true => {
                let formated_genres: String = self
                    .genres
                    .clone()
                    .into_iter()
                    .map(|genre| genre.format())
                    .collect::<Vec<String>>()
                    .join("\n");
                format!("Жанры:\n{formated_genres}\n\n")
            }
            false => "".to_string(),
        };

        let links: String = self
            .available_types
            .clone()
            .into_iter()
            .map(|a_type| {
                let Book { id, .. } = self;
                format!("📥 {a_type}: /d_{a_type}_{id}")
            })
            .collect::<Vec<String>>()
            .join("\n");
        let download_links = format!("Скачать:\n{links}");

        format!("{book_title}{pages_count}{annotations}{authors}{translators}{sequences}{genres}{download_links}")
    }
}

impl Format for Author {
    fn format(&self) -> String {
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
    fn format(&self) -> String {
        let Sequence { id, name, .. } = self;

        let title = format!("📚 {name}\n");
        let link = format!("/s_{id}");

        format!("{title}{link}")
    }
}

impl Format for SearchBook {
    fn format(&self) -> String {
        let book_title = {
            let SearchBook { title, lang, .. } = self;
            format!("📖 {title} | {lang}\n")
        };

        let annotations = match self.annotation_exists {
            true => {
                let SearchBook { id, .. } = self;
                format!("📝 Аннотация: /b_an_{id}\n")
            }
            false => "".to_string(),
        };

        let authors = match self.authors.len() != 0 {
            true => {
                let formated_authors = self
                    .authors
                    .clone()
                    .into_iter()
                    .map(|author| author.format_author())
                    .collect::<Vec<String>>()
                    .join("\n");
                format!("Авторы:\n{formated_authors}\n")
            }
            false => "".to_string(),
        };

        let translators = match self.translators.len() != 0 {
            true => {
                let formated_translators = self
                    .translators
                    .clone()
                    .into_iter()
                    .map(|translator| translator.format_translator())
                    .collect::<Vec<String>>()
                    .join("\n");
                format!("Переводчики:\n{formated_translators}\n")
            }
            false => "".to_string(),
        };

        let links: String = self
            .available_types
            .clone()
            .into_iter()
            .map(|a_type| {
                let SearchBook { id, .. } = self;
                format!("📥 {a_type}: /d_{a_type}_{id}")
            })
            .collect::<Vec<String>>()
            .join("\n");
        let download_links = format!("Скачать:\n{links}");

        format!("{book_title}{annotations}{authors}{translators}{download_links}")
    }
}

impl Format for Translator {
    fn format(&self) -> String {
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

impl Format for AuthorBook {
    fn format(&self) -> String {
        let book_title = {
            let AuthorBook { title, lang, .. } = self;
            format!("📖 {title} | {lang}\n")
        };

        let annotations = match self.annotation_exists {
            true => {
                let AuthorBook { id, .. } = self;
                format!("📝 Аннотация: /b_an_{id}\n")
            }
            false => "".to_string(),
        };

        let translators = match self.translators.len() != 0 {
            true => {
                let formated_translators = self
                    .translators
                    .clone()
                    .into_iter()
                    .map(|translator| translator.format_translator())
                    .collect::<Vec<String>>()
                    .join("\n");
                format!("Переводчики:\n{formated_translators}\n")
            }
            false => "".to_string(),
        };

        let links: String = self
            .available_types
            .clone()
            .into_iter()
            .map(|a_type| {
                let AuthorBook { id, .. } = self;
                format!("📥 {a_type}: /d_{a_type}_{id}")
            })
            .collect::<Vec<String>>()
            .join("\n");
        let download_links = format!("Скачать:\n{links}");

        format!("{book_title}{annotations}{translators}{download_links}")
    }
}

impl Format for TranslatorBook {
    fn format(&self) -> String {
        let book_title = {
            let TranslatorBook { title, lang, .. } = self;
            format!("📖 {title} | {lang}\n")
        };

        let annotations = match self.annotation_exists {
            true => {
                let TranslatorBook { id, .. } = self;
                format!("📝 Аннотация: /b_an_{id}\n")
            }
            false => "".to_string(),
        };

        let authors = match self.authors.len() != 0 {
            true => {
                let formated_authors = self
                    .authors
                    .clone()
                    .into_iter()
                    .map(|author| author.format_author())
                    .collect::<Vec<String>>()
                    .join("\n");
                format!("Авторы:\n{formated_authors}\n")
            }
            false => "".to_string(),
        };

        let links: String = self
            .available_types
            .clone()
            .into_iter()
            .map(|a_type| {
                let TranslatorBook { id, .. } = self;
                format!("📥 {a_type}: /d_{a_type}_{id}")
            })
            .collect::<Vec<String>>()
            .join("\n");
        let download_links = format!("Скачать:\n{links}");

        format!("{book_title}{annotations}{authors}{download_links}")
    }
}
