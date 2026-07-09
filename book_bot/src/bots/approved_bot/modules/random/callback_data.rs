use std::fmt::Display;

#[derive(Clone)]
pub enum RandomCallbackData {
    RandomBook,
    RandomAuthor,
    RandomSequence,
    RandomBookByGenreRequest,
    Genres { index: u32 },
    RandomBookByGenre { id: u32 },
}

impl Display for RandomCallbackData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RandomCallbackData::RandomBook => write!(f, "random_book"),
            RandomCallbackData::RandomAuthor => write!(f, "random_author"),
            RandomCallbackData::RandomSequence => write!(f, "random_sequence"),
            RandomCallbackData::RandomBookByGenreRequest => {
                write!(f, "random_book_by_genre_request")
            }
            RandomCallbackData::Genres { index } => write!(f, "genres_{index}"),
            RandomCallbackData::RandomBookByGenre { id } => {
                write!(f, "random_book_by_genre_{id}")
            }
        }
    }
}

impl std::str::FromStr for RandomCallbackData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "random_book" => return Ok(RandomCallbackData::RandomBook),
            "random_author" => return Ok(RandomCallbackData::RandomAuthor),
            "random_sequence" => return Ok(RandomCallbackData::RandomSequence),
            "random_book_by_genre_request" => {
                return Ok(RandomCallbackData::RandomBookByGenreRequest)
            }
            _ => {}
        }

        if let Some(suffix) = s.strip_prefix("genres_") {
            let index: u32 = suffix
                .parse()
                .map_err(|_| strum::ParseError::VariantNotFound)?;
            return Ok(RandomCallbackData::Genres { index });
        }

        if let Some(suffix) = s.strip_prefix("random_book_by_genre_") {
            let id: u32 = suffix
                .parse()
                .map_err(|_| strum::ParseError::VariantNotFound)?;
            return Ok(RandomCallbackData::RandomBookByGenre { id });
        }

        Err(strum::ParseError::VariantNotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::RandomCallbackData;
    use std::str::FromStr;

    #[test]
    fn round_trip_random_book() {
        match RandomCallbackData::from_str(&RandomCallbackData::RandomBook.to_string()).unwrap() {
            RandomCallbackData::RandomBook => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_random_author() {
        match RandomCallbackData::from_str(&RandomCallbackData::RandomAuthor.to_string()).unwrap() {
            RandomCallbackData::RandomAuthor => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_random_sequence() {
        match RandomCallbackData::from_str(&RandomCallbackData::RandomSequence.to_string()).unwrap()
        {
            RandomCallbackData::RandomSequence => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_random_book_by_genre_request() {
        match RandomCallbackData::from_str(
            &RandomCallbackData::RandomBookByGenreRequest.to_string(),
        )
        .unwrap()
        {
            RandomCallbackData::RandomBookByGenreRequest => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_genres() {
        let cd = RandomCallbackData::Genres { index: 7 };
        match RandomCallbackData::from_str(&cd.to_string()).unwrap() {
            RandomCallbackData::Genres { index } => assert_eq!(index, 7),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_random_book_by_genre() {
        let cd = RandomCallbackData::RandomBookByGenre { id: 42 };
        match RandomCallbackData::from_str(&cd.to_string()).unwrap() {
            RandomCallbackData::RandomBookByGenre { id } => assert_eq!(id, 42),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn genres_display_includes_index() {
        assert_eq!(
            RandomCallbackData::Genres { index: 3 }.to_string(),
            "genres_3"
        );
    }

    #[test]
    fn random_book_by_genre_display_includes_id() {
        assert_eq!(
            RandomCallbackData::RandomBookByGenre { id: 9 }.to_string(),
            "random_book_by_genre_9"
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(RandomCallbackData::from_str("not_a_thing").is_err());
    }

    #[test]
    fn rejects_genres_without_index() {
        assert!(RandomCallbackData::from_str("genres_").is_err());
        assert!(RandomCallbackData::from_str("genres_abc").is_err());
    }
}
