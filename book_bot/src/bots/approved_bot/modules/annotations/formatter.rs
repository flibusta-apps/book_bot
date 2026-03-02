use crate::bots::approved_bot::services::book_library::types::{AuthorAnnotation, BookAnnotation};

pub trait AnnotationFormat {
    fn get_file(&self) -> Option<&String>;
    fn get_text(&self) -> &str;

    fn is_normal_text(&self) -> bool;
}

impl AnnotationFormat for BookAnnotation {
    fn get_file(&self) -> Option<&String> {
        self.file.as_ref()
    }

    fn get_text(&self) -> &str {
        self.text.as_str()
    }

    fn is_normal_text(&self) -> bool {
        !self.text.replace(['\n', ' '], "").is_empty()
    }
}

impl AnnotationFormat for AuthorAnnotation {
    fn get_file(&self) -> Option<&String> {
        self.file.as_ref()
    }

    fn get_text(&self) -> &str {
        self.text.as_str()
    }

    fn is_normal_text(&self) -> bool {
        !self.text.replace(['\n', ' '], "").is_empty()
    }
}
