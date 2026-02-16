use std::{fmt::Display, str::FromStr};

use regex::Regex;
use smartstring::alias::String as SmartString;

#[derive(Clone)]
pub enum SettingsCallbackData {
    Settings,
    On {
        code: SmartString,
    },
    Off {
        code: SmartString,
    },
    /// Open "default search type" submenu
    DefaultSearchMenu,
    /// Set default search: value is "book"|"author"|"series"|"translator"|"none"
    DefaultSearch {
        value: SmartString,
    },
    /// Return from default search submenu to main settings
    DefaultSearchBack,
}

impl FromStr for SettingsCallbackData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == SettingsCallbackData::Settings.to_string().as_str() {
            return Ok(SettingsCallbackData::Settings);
        }
        if s == "defsearch" {
            return Ok(SettingsCallbackData::DefaultSearchMenu);
        }
        if s == "defsearch_back" {
            return Ok(SettingsCallbackData::DefaultSearchBack);
        }
        if let Some(value) = s.strip_prefix("defsearch_") {
            return Ok(SettingsCallbackData::DefaultSearch {
                value: value.to_string().into(),
            });
        }

        let re = Regex::new(r"^lang_(?P<action>(off)|(on))_(?P<code>[a-zA-z]+)$").unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let action = &caps["action"];
        let code = caps["code"].to_string();

        match action {
            "on" => Ok(SettingsCallbackData::On { code: code.into() }),
            "off" => Ok(SettingsCallbackData::Off { code: code.into() }),
            _ => Err(strum::ParseError::VariantNotFound),
        }
    }
}

impl Display for SettingsCallbackData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingsCallbackData::Settings => write!(f, "lang_settings"),
            SettingsCallbackData::On { code } => write!(f, "lang_on_{code}"),
            SettingsCallbackData::Off { code } => write!(f, "lang_off_{code}"),
            SettingsCallbackData::DefaultSearchMenu => write!(f, "defsearch"),
            SettingsCallbackData::DefaultSearch { value } => write!(f, "defsearch_{value}"),
            SettingsCallbackData::DefaultSearchBack => write!(f, "defsearch_back"),
        }
    }
}
