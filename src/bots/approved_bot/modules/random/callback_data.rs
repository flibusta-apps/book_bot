use strum_macros::{EnumIter, Display};


#[derive(Clone, Display, EnumIter)]
#[strum(serialize_all = "snake_case")]
pub enum RandomCallbackData {
    RandomBook,
    RandomAuthor,
    RandomSequence,
    RandomBookByGenreRequest,
    Genres { index: u32 },
    RandomBookByGenre { id: u32 },
}

impl std::str::FromStr for RandomCallbackData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s.to_string();

        for callback_data in <RandomCallbackData as strum::IntoEnumIterator>::iter() {
            match callback_data {
                RandomCallbackData::Genres { .. }
                | RandomCallbackData::RandomBookByGenre { .. } => {
                    let callback_prefix = callback_data.to_string();

                    if value.starts_with(&callback_prefix) {
                        let data: u32 = value
                            .strip_prefix(&format!("{}_", &callback_prefix).to_string())
                            .unwrap()
                            .parse()
                            .unwrap();

                        match callback_data {
                            RandomCallbackData::Genres { .. } => {
                                return Ok(RandomCallbackData::Genres { index: data })
                            }
                            RandomCallbackData::RandomBookByGenre { .. } => {
                                return Ok(RandomCallbackData::RandomBookByGenre { id: data })
                            }
                            _ => (),
                        }
                    }
                }
                _ => {
                    if value == callback_data.to_string() {
                        return Ok(callback_data);
                    }
                }
            }
        }

        Err(strum::ParseError::VariantNotFound)
    }
}
