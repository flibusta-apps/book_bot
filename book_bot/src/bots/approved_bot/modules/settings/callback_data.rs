use std::{fmt::Display, str::FromStr};

use regex::Regex;
use smartstring::alias::String as SmartString;
use std::sync::LazyLock;

static RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^lang_(?P<action>(off)|(on))_(?P<code>[a-zA-z]+)$").unwrap());

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
