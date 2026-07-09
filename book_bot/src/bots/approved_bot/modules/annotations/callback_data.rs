use std::{fmt::Display, str::FromStr};

use regex::Regex;
use std::sync::LazyLock;

use crate::bots::approved_bot::modules::utils::{
    errors::CallbackQueryParseError, pagination::GetPaginationCallbackData,
};

static RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?P<an_type>[ab])_an_(?P<id>\d+)_(?P<page>\d+)$").unwrap());

#[derive(Debug, Clone)]
pub enum AnnotationCallbackData {
    Book { id: u32, page: u32 },
    Author { id: u32, page: u32 },
}

impl FromStr for AnnotationCallbackData {
    type Err = CallbackQueryParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let caps = RE.captures(s).ok_or(CallbackQueryParseError)?;

        let an_type = &caps["an_type"];
        let id: u32 = caps["id"].parse().map_err(|_| CallbackQueryParseError)?;
        let page: u32 = std::cmp::max(
            1,
            caps["page"]
                .parse::<u32>()
                .map_err(|_| CallbackQueryParseError)?,
        );

        match an_type {
            "a" => Ok(AnnotationCallbackData::Author { id, page }),
            "b" => Ok(AnnotationCallbackData::Book { id, page }),
            _ => Err(CallbackQueryParseError),
        }
    }
}

impl Display for AnnotationCallbackData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnnotationCallbackData::Book { id, page } => write!(f, "b_an_{id}_{page}"),
            AnnotationCallbackData::Author { id, page } => write!(f, "a_an_{id}_{page}"),
        }
    }
}

impl GetPaginationCallbackData for AnnotationCallbackData {
    fn get_pagination_callback_data(&self, target_page: u32) -> String {
        match self {
            AnnotationCallbackData::Book { id, .. } => AnnotationCallbackData::Book {
                id: *id,
                page: target_page,
            },
            AnnotationCallbackData::Author { id, .. } => AnnotationCallbackData::Author {
                id: *id,
                page: target_page,
            },
        }
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::AnnotationCallbackData;
    use std::str::FromStr;

    #[test]
    fn page_zero_normalized_to_one_book() {
        let cd = AnnotationCallbackData::from_str("b_an_5_0").unwrap();
        match cd {
            AnnotationCallbackData::Book { page, .. } => assert_eq!(page, 1),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn page_zero_normalized_to_one_author() {
        let cd = AnnotationCallbackData::from_str("a_an_5_0").unwrap();
        match cd {
            AnnotationCallbackData::Author { page, .. } => assert_eq!(page, 1),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn normal_page_preserved() {
        let cd = AnnotationCallbackData::from_str("b_an_42_3").unwrap();
        match cd {
            AnnotationCallbackData::Book { id, page } => {
                assert_eq!(id, 42);
                assert_eq!(page, 3);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_book() {
        let cd = AnnotationCallbackData::Book { id: 10, page: 2 };
        match AnnotationCallbackData::from_str(&cd.to_string()).unwrap() {
            AnnotationCallbackData::Book { id, page } => {
                assert_eq!(id, 10);
                assert_eq!(page, 2);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_author() {
        let cd = AnnotationCallbackData::Author { id: 11, page: 5 };
        match AnnotationCallbackData::from_str(&cd.to_string()).unwrap() {
            AnnotationCallbackData::Author { id, page } => {
                assert_eq!(id, 11);
                assert_eq!(page, 5);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rejects_foreign_prefix() {
        assert!(AnnotationCallbackData::from_str("x_an_5_1").is_err());
    }

    #[test]
    fn rejects_non_numeric_id() {
        assert!(AnnotationCallbackData::from_str("b_an_abc_1").is_err());
    }
}
