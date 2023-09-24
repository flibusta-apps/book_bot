use std::str::FromStr;

use chrono::NaiveDate;
use dateparser::parse;
use regex::Regex;

use crate::bots::approved_bot::modules::utils::pagination::GetPaginationCallbackData;

#[derive(Clone, Copy)]
pub struct UpdateLogCallbackData {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub page: u32,
}

impl FromStr for UpdateLogCallbackData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(
            r"^update_log_(?P<from>\d{4}-\d{2}-\d{2})_(?P<to>\d{4}-\d{2}-\d{2})_(?P<page>\d+)$",
        )
        .unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let from: NaiveDate = parse(&caps["from"]).unwrap().date_naive();
        let to: NaiveDate = parse(&caps["to"]).unwrap().date_naive();
        let page: u32 = caps["page"].parse().unwrap();

        Ok(UpdateLogCallbackData { from, to, page })
    }
}

impl ToString for UpdateLogCallbackData {
    fn to_string(&self) -> String {
        let date_format = "%Y-%m-%d";

        let from = self.from.format(date_format);
        let to = self.to.format(date_format);
        let page = self.page;

        format!("update_log_{from}_{to}_{page}")
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
