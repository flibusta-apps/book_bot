use std::{fmt::Display, str::FromStr};

use regex::Regex;
use smartstring::alias::String as SmartString;
use std::sync::LazyLock;

static RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^lang_(?P<action>(off)|(on))_(?P<code>[a-zA-Z]+)$").unwrap());

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
    /// Return from languages submenu to main settings
    LangSettingsBack,
    /// Open "file name language" submenu
    FileNameLangMenu,
    /// Set file name language: value is "normalized"|"original"
    FileNameLang {
        value: SmartString,
    },
    /// Return from file name language submenu to main settings
    FileNameLangBack,
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
        if s == "lang_settings_back" {
            return Ok(SettingsCallbackData::LangSettingsBack);
        }
        if s == "filename_lang" {
            return Ok(SettingsCallbackData::FileNameLangMenu);
        }
        if s == "filename_lang_back" {
            return Ok(SettingsCallbackData::FileNameLangBack);
        }
        if let Some(value) = s.strip_prefix("defsearch_") {
            return Ok(SettingsCallbackData::DefaultSearch {
                value: value.to_string().into(),
            });
        }
        if let Some(value) = s.strip_prefix("filename_lang_") {
            return Ok(SettingsCallbackData::FileNameLang {
                value: value.to_string().into(),
            });
        }

        let caps = RE.captures(s).ok_or(strum::ParseError::VariantNotFound)?;

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
            SettingsCallbackData::LangSettingsBack => write!(f, "lang_settings_back"),
            SettingsCallbackData::FileNameLangMenu => write!(f, "filename_lang"),
            SettingsCallbackData::FileNameLang { value } => write!(f, "filename_lang_{value}"),
            SettingsCallbackData::FileNameLangBack => write!(f, "filename_lang_back"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SettingsCallbackData;
    use std::str::FromStr;

    #[test]
    fn round_trip_settings_menu() {
        match SettingsCallbackData::from_str(&SettingsCallbackData::Settings.to_string()).unwrap() {
            SettingsCallbackData::Settings => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_on() {
        let cd = SettingsCallbackData::On { code: "ru".into() };
        match SettingsCallbackData::from_str(&cd.to_string()).unwrap() {
            SettingsCallbackData::On { code } => assert_eq!(code, "ru"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_off() {
        let cd = SettingsCallbackData::Off { code: "en".into() };
        match SettingsCallbackData::from_str(&cd.to_string()).unwrap() {
            SettingsCallbackData::Off { code } => assert_eq!(code, "en"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_default_search_menu() {
        match SettingsCallbackData::from_str(&SettingsCallbackData::DefaultSearchMenu.to_string())
            .unwrap()
        {
            SettingsCallbackData::DefaultSearchMenu => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_default_search() {
        let cd = SettingsCallbackData::DefaultSearch {
            value: "book".into(),
        };
        match SettingsCallbackData::from_str(&cd.to_string()).unwrap() {
            SettingsCallbackData::DefaultSearch { value } => assert_eq!(value, "book"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_default_search_back() {
        match SettingsCallbackData::from_str(&SettingsCallbackData::DefaultSearchBack.to_string())
            .unwrap()
        {
            SettingsCallbackData::DefaultSearchBack => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_lang_settings_back() {
        match SettingsCallbackData::from_str(&SettingsCallbackData::LangSettingsBack.to_string())
            .unwrap()
        {
            SettingsCallbackData::LangSettingsBack => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_file_name_lang_menu() {
        match SettingsCallbackData::from_str(&SettingsCallbackData::FileNameLangMenu.to_string())
            .unwrap()
        {
            SettingsCallbackData::FileNameLangMenu => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_file_name_lang() {
        let cd = SettingsCallbackData::FileNameLang {
            value: "original".into(),
        };
        match SettingsCallbackData::from_str(&cd.to_string()).unwrap() {
            SettingsCallbackData::FileNameLang { value } => assert_eq!(value, "original"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_file_name_lang_back() {
        match SettingsCallbackData::from_str(&SettingsCallbackData::FileNameLangBack.to_string())
            .unwrap()
        {
            SettingsCallbackData::FileNameLangBack => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn accepts_multi_letter_language_code() {
        match SettingsCallbackData::from_str("lang_on_eng").unwrap() {
            SettingsCallbackData::On { code } => assert_eq!(code, "eng"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rejects_garbage_language_code() {
        assert!(SettingsCallbackData::from_str("lang_on__").is_err());
    }

    #[test]
    fn rejects_foreign_prefix() {
        assert!(SettingsCallbackData::from_str("totally_unknown").is_err());
    }
}
