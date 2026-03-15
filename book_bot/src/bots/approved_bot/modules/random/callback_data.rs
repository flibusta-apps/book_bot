use strum_macros::{Display, EnumIter};

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
        for callback_data in <RandomCallbackData as strum::IntoEnumIterator>::iter() {
            match callback_data {
                RandomCallbackData::Genres { .. }
                | RandomCallbackData::RandomBookByGenre { .. } => {
                    let callback_prefix = callback_data.to_string();

                    if let Some(suffix) = s.strip_prefix(&callback_prefix) {
                        let data: u32 = suffix
                            .strip_prefix('_')
                            .ok_or(strum::ParseError::VariantNotFound)?
                            .parse()
                            .map_err(|_| strum::ParseError::VariantNotFound)?;

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
                    if s == callback_data.to_string() {
                        return Ok(callback_data);
                    }
                }
            }
        }

        Err(strum::ParseError::VariantNotFound)
    }
}
