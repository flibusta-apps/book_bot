use std::{fmt::Display, str::FromStr};

use regex::Regex;
use std::sync::LazyLock;

use crate::bots::approved_bot::modules::utils::{
    errors::CallbackQueryParseError, pagination::GetPaginationCallbackData,
};

static RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^b(?P<an_type>[ats])_(?P<id>\d+)_(?P<page>\d+)$").unwrap());

#[derive(Clone)]
pub enum BookCallbackData {
    Author { id: u32, page: u32 },
    Translator { id: u32, page: u32 },
    Sequence { id: u32, page: u32 },
}

impl FromStr for BookCallbackData {
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
            "a" => Ok(BookCallbackData::Author { id, page }),
            "t" => Ok(BookCallbackData::Translator { id, page }),
            "s" => Ok(BookCallbackData::Sequence { id, page }),
            _ => Err(CallbackQueryParseError),
        }
    }
}

impl Display for BookCallbackData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BookCallbackData::Author { id, page } => write!(f, "ba_{id}_{page}"),
            BookCallbackData::Translator { id, page } => write!(f, "bt_{id}_{page}"),
            BookCallbackData::Sequence { id, page } => write!(f, "bs_{id}_{page}"),
        }
    }
}

impl GetPaginationCallbackData for BookCallbackData {
    fn get_pagination_callback_data(&self, target_page: u32) -> String {
        match self {
            BookCallbackData::Author { id, .. } => BookCallbackData::Author {
                id: *id,
                page: target_page,
            },
            BookCallbackData::Translator { id, .. } => BookCallbackData::Translator {
                id: *id,
                page: target_page,
            },
            BookCallbackData::Sequence { id, .. } => BookCallbackData::Sequence {
                id: *id,
                page: target_page,
            },
        }
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::BookCallbackData;
    use std::str::FromStr;

    #[test]
    fn page_zero_normalized_to_one() {
        let cd = BookCallbackData::from_str("ba_5_0").unwrap();
        match cd {
            BookCallbackData::Author { page, .. } => assert_eq!(page, 1),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn normal_page_preserved() {
        let cd = BookCallbackData::from_str("bs_7_4").unwrap();
        match cd {
            BookCallbackData::Sequence { id, page } => {
                assert_eq!(id, 7);
                assert_eq!(page, 4);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_author() {
        let cd = BookCallbackData::Author { id: 1, page: 2 };
        match BookCallbackData::from_str(&cd.to_string()).unwrap() {
            BookCallbackData::Author { id, page } => {
                assert_eq!(id, 1);
                assert_eq!(page, 2);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_translator() {
        let cd = BookCallbackData::Translator { id: 3, page: 4 };
        match BookCallbackData::from_str(&cd.to_string()).unwrap() {
            BookCallbackData::Translator { id, page } => {
                assert_eq!(id, 3);
                assert_eq!(page, 4);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_sequence() {
        let cd = BookCallbackData::Sequence { id: 5, page: 6 };
        match BookCallbackData::from_str(&cd.to_string()).unwrap() {
            BookCallbackData::Sequence { id, page } => {
                assert_eq!(id, 5);
                assert_eq!(page, 6);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rejects_foreign_prefix() {
        assert!(BookCallbackData::from_str("bx_5_1").is_err());
    }

    #[test]
    fn rejects_non_numeric_id() {
        assert!(BookCallbackData::from_str("ba_abc_1").is_err());
    }
}
