use std::str::FromStr;

use regex::Regex;
use smartstring::alias::String as SmartString;


#[derive(Clone)]
pub enum SettingsCallbackData {
    Settings,
    On { code: SmartString },
    Off { code: SmartString },
}

impl FromStr for SettingsCallbackData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == SettingsCallbackData::Settings.to_string().as_str() {
            return Ok(SettingsCallbackData::Settings);
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

impl ToString for SettingsCallbackData {
    fn to_string(&self) -> String {
        match self {
            SettingsCallbackData::Settings => "lang_settings".to_string(),
            SettingsCallbackData::On { code } => format!("lang_on_{code}"),
            SettingsCallbackData::Off { code } => format!("lang_off_{code}"),
        }
    }
}
