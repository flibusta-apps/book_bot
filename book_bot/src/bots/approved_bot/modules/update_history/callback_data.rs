use std::{fmt::Display, str::FromStr};

use chrono::NaiveDate;
use dateparser::parse;
use regex::Regex;
use std::sync::LazyLock;

use crate::bots::approved_bot::modules::utils::pagination::GetPaginationCallbackData;

static RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^update_log_(?P<from>\d{4}-\d{2}-\d{2})_(?P<to>\d{4}-\d{2}-\d{2})_(?P<page>\d+)$")
        .unwrap()
});

#[derive(Clone, Copy)]
pub struct UpdateLogCallbackData {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub page: u32,
}

impl FromStr for UpdateLogCallbackData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let caps = RE.captures(s).ok_or(strum::ParseError::VariantNotFound)?;

        let from: NaiveDate = parse(&caps["from"])
            .map_err(|_| strum::ParseError::VariantNotFound)?
            .date_naive();
        let to: NaiveDate = parse(&caps["to"])
            .map_err(|_| strum::ParseError::VariantNotFound)?
            .date_naive();
        let page: u32 = caps["page"]
            .parse()
            .map_err(|_| strum::ParseError::VariantNotFound)?;
        let page: u32 = std::cmp::max(1, page);

        Ok(UpdateLogCallbackData { from, to, page })
    }
}

impl Display for UpdateLogCallbackData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let date_format = "%Y-%m-%d";

        let from = self.from.format(date_format);
        let to = self.to.format(date_format);
        let page = self.page;

        write!(f, "update_log_{from}_{to}_{page}")
    }
}

impl GetPaginationCallbackData for UpdateLogCallbackData {
    fn get_pagination_callback_data(&self, target_page: u32) -> String {
        let UpdateLogCallbackData { from, to, .. } = self;
        UpdateLogCallbackData {
            from: *from,
            to: *to,
            page: target_page,
        }
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::UpdateLogCallbackData;
    use chrono::NaiveDate;
    use std::str::FromStr;

    #[test]
    fn round_trip() {
        let cd = UpdateLogCallbackData {
            from: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            to: NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
            page: 3,
        };
        let parsed = UpdateLogCallbackData::from_str(&cd.to_string()).unwrap();
        assert_eq!(parsed.from, cd.from);
        assert_eq!(parsed.to, cd.to);
        assert_eq!(parsed.page, cd.page);
    }

    #[test]
    fn page_zero_normalized_to_one() {
        let parsed = UpdateLogCallbackData::from_str("update_log_2024-01-01_2024-01-31_0").unwrap();
        assert_eq!(parsed.page, 1);
    }

    #[test]
    fn rejects_garbage() {
        assert!(UpdateLogCallbackData::from_str("not_a_thing").is_err());
    }

    #[test]
    fn rejects_invalid_date() {
        assert!(UpdateLogCallbackData::from_str("update_log_bad-date_2024-01-31_1").is_err());
    }
}
