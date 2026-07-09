# Callback Data Round-Trip Tests Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the four callback-data/command parser bugs identified in `docs/specs/09-callback-data-roundtrip-tests.md` and add round-trip/negative-case unit tests to all 7 `callback_data.rs` modules and the 3 `commands.rs` modules that hand-parse `@botname` mentions.

**Architecture:** No new abstractions — this is targeted bug-fixing plus `#[cfg(test)] mod tests` blocks in the existing files, following the match-based assertion style already used in `book/callback_data.rs` and `annotations/callback_data.rs`. The one structural change is centralizing `@botname` stripping into a single case-insensitive helper in `utils/filter_command.rs`, called once by `filter_command()`, so the three `CommandParse` implementors stop hand-rolling case-sensitive stripping.

**Tech Stack:** Rust, `regex`, `strum`/`strum_macros`, `smartstring`, `chrono`. Workspace root: `/Users/kurbezz/Projects/books_project/book_bot`. Crate root: `book_bot/` (package name `book_bot`, binary crate — no `--lib` target, run tests with `cargo test -p book_bot <filter>` from the workspace root).

## Global Constraints

- Do not change any wire format that is not explicitly called out as buggy — old callback data already sitting in years-old Telegram messages must keep parsing (e.g. `SearchCallbackData`, `BookCallbackData`, `AnnotationCallbackData`, `DownloadQueryData`, `DownloadArchiveQueryData`, `CheckArchiveStatus` string formats are unchanged).
- Follow the existing test style in this codebase: plain `#[cfg(test)] mod tests` with `match` + field assertions (no new `PartialEq`/`Debug` derives), matching `book/callback_data.rs:74-99` and `annotations/callback_data.rs:67-101`.
- Every task must leave `cargo test -p book_bot` green before it is committed.

---

### Task 1: Centralize case-insensitive `@botname` stripping

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/utils/filter_command.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/annotations/commands.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/download/commands.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/book/commands.rs`

**Interfaces:**
- Produces: `pub fn strip_bot_mention(text: &str, bot_name: &str) -> String` in `filter_command.rs`, case-insensitive, removes every occurrence of `@bot_name` (any case combination), no-op when `bot_name` is empty.
- Produces: `pub trait CommandParse<T> { fn parse(s: &str) -> Result<T, CommandParseError>; }` — the `bot_name` parameter is removed; callers must pre-strip via `strip_bot_mention` before calling `parse`.

- [ ] **Step 1: Write the failing tests for `strip_bot_mention`**

Add to the bottom of `book_bot/src/bots/approved_bot/modules/utils/filter_command.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::strip_bot_mention;

    #[test]
    fn strips_matching_case() {
        assert_eq!(strip_bot_mention("/d_1@MyBot", "MyBot"), "/d_1");
    }

    #[test]
    fn strips_case_insensitive_lower_bot_name() {
        assert_eq!(strip_bot_mention("/d_1@MyBot", "mybot"), "/d_1");
    }

    #[test]
    fn strips_case_insensitive_lower_mention() {
        assert_eq!(strip_bot_mention("/d_1@mybot", "MyBot"), "/d_1");
    }

    #[test]
    fn leaves_text_without_mention_unchanged() {
        assert_eq!(strip_bot_mention("/d_1", "MyBot"), "/d_1");
    }

    #[test]
    fn empty_bot_name_is_a_no_op() {
        assert_eq!(strip_bot_mention("/d_1@MyBot", ""), "/d_1@MyBot");
    }

    #[test]
    fn strips_every_occurrence() {
        assert_eq!(
            strip_bot_mention("/d_1@MyBot text @MyBot more", "mybot"),
            "/d_1 text  more"
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (from the workspace root `/Users/kurbezz/Projects/books_project/book_bot`):

```bash
cargo test -p book_bot strip_bot_mention
```

Expected: compile error — `cannot find function 'strip_bot_mention' in this scope`.

- [ ] **Step 3: Implement `strip_bot_mention`**

Add above the existing `filter_command` function in `book_bot/src/bots/approved_bot/modules/utils/filter_command.rs`:

```rust
pub fn strip_bot_mention(text: &str, bot_name: &str) -> String {
    if bot_name.is_empty() {
        return text.to_string();
    }

    let mention = format!("@{bot_name}");
    let lower_text = text.to_ascii_lowercase();
    let lower_mention = mention.to_ascii_lowercase();

    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    let mut lower_rest = lower_text.as_str();

    while let Some(pos) = lower_rest.find(&lower_mention) {
        result.push_str(&rest[..pos]);
        rest = &rest[pos + mention.len()..];
        lower_rest = &lower_rest[pos + mention.len()..];
    }
    result.push_str(rest);

    result
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p book_bot strip_bot_mention`
Expected: `test result: ok. 6 passed; 0 failed`

- [ ] **Step 5: Wire the helper into `filter_command()` and simplify the `CommandParse` trait**

Replace the trait and function in `book_bot/src/bots/approved_bot/modules/utils/filter_command.rs` (everything above the new `#[cfg(test)]` block) with:

```rust
use teloxide::{dptree, prelude::*, types::*};

use super::errors::CommandParseError;

pub trait CommandParse<T> {
    fn parse(s: &str) -> Result<T, CommandParseError>;
}

pub fn strip_bot_mention(text: &str, bot_name: &str) -> String {
    if bot_name.is_empty() {
        return text.to_string();
    }

    let mention = format!("@{bot_name}");
    let lower_text = text.to_ascii_lowercase();
    let lower_mention = mention.to_ascii_lowercase();

    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    let mut lower_rest = lower_text.as_str();

    while let Some(pos) = lower_rest.find(&lower_mention) {
        result.push_str(&rest[..pos]);
        rest = &rest[pos + mention.len()..];
        lower_rest = &lower_rest[pos + mention.len()..];
    }
    result.push_str(rest);

    result
}

pub fn filter_command<Output>() -> crate::bots::BotHandler
where
    Output: CommandParse<Output> + Send + Sync + 'static,
{
    dptree::entry().chain(dptree::filter_map(move |message: Message, me: Me| {
        let bot_name = me.user.username.unwrap_or_default();
        message.text().and_then(|text| {
            let normalized = strip_bot_mention(text, &bot_name);
            Output::parse(&normalized).ok()
        })
    }))
}
```

Then update the three implementors so the crate compiles again.

`book_bot/src/bots/approved_bot/modules/annotations/commands.rs` — replace the `impl CommandParse<Self> for AnnotationCommand` block with:

```rust
impl CommandParse<Self> for AnnotationCommand {
    fn parse(s: &str) -> Result<Self, CommandParseError> {
        let caps = RE.captures(s).ok_or(CommandParseError)?;

        let an_type = &caps["an_type"];
        let id: u32 = caps["id"].parse().map_err(|_| CommandParseError)?;

        match an_type {
            "a" => Ok(AnnotationCommand::Author { id }),
            "b" => Ok(AnnotationCommand::Book { id }),
            _ => Err(CommandParseError),
        }
    }
}
```

`book_bot/src/bots/approved_bot/modules/download/commands.rs` — replace both `impl CommandParse<Self> for ...` blocks with:

```rust
impl CommandParse<Self> for StartDownloadCommand {
    fn parse(s: &str) -> Result<Self, CommandParseError> {
        let caps = RE_DOWNLOAD.captures(s).ok_or(CommandParseError)?;

        let book_id: u32 = caps["book_id"].parse().map_err(|_| CommandParseError)?;

        Ok(StartDownloadCommand { id: book_id })
    }
}
```

```rust
impl CommandParse<Self> for DownloadArchiveCommand {
    fn parse(s: &str) -> Result<Self, CommandParseError> {
        let caps = RE_ARCHIVE.captures(s).ok_or(CommandParseError)?;

        let id: u32 = caps["id"].parse().map_err(|_| CommandParseError)?;

        match &caps["type"] {
            "s" => Ok(DownloadArchiveCommand::Sequence { id }),
            "a" => Ok(DownloadArchiveCommand::Author { id }),
            "t" => Ok(DownloadArchiveCommand::Translator { id }),
            _ => Err(CommandParseError),
        }
    }
}
```

`book_bot/src/bots/approved_bot/modules/book/commands.rs` — replace the `impl CommandParse<Self> for BookCommand` block with:

```rust
impl CommandParse<Self> for BookCommand {
    fn parse(s: &str) -> Result<Self, CommandParseError> {
        let caps = RE.captures(s).ok_or(CommandParseError)?;

        let an_type = &caps["an_type"];
        let id: u32 = caps["id"].parse().map_err(|_| CommandParseError)?;

        match an_type {
            "a" => Ok(BookCommand::Author { id }),
            "t" => Ok(BookCommand::Translator { id }),
            "s" => Ok(BookCommand::Sequence { id }),
            _ => Err(CommandParseError),
        }
    }
}
```

- [ ] **Step 6: Add parsing tests to the three command files**

Append to `book_bot/src/bots/approved_bot/modules/annotations/commands.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::AnnotationCommand;

    #[test]
    fn parses_book() {
        match AnnotationCommand::parse("/b_an_5").unwrap() {
            AnnotationCommand::Book { id } => assert_eq!(id, 5),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_author() {
        match AnnotationCommand::parse("/a_an_7").unwrap() {
            AnnotationCommand::Author { id } => assert_eq!(id, 7),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rejects_foreign_prefix() {
        assert!(AnnotationCommand::parse("/x_an_5").is_err());
    }

    #[test]
    fn rejects_non_numeric_id() {
        assert!(AnnotationCommand::parse("/b_an_abc").is_err());
    }
}
```

Append to `book_bot/src/bots/approved_bot/modules/download/commands.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::{DownloadArchiveCommand, StartDownloadCommand};
    use crate::bots::approved_bot::modules::utils::filter_command::strip_bot_mention;

    #[test]
    fn round_trip_start_download() {
        let cmd = StartDownloadCommand { id: 5 };
        let parsed = StartDownloadCommand::parse(&cmd.to_string()).unwrap();
        assert_eq!(parsed.id, 5);
    }

    #[test]
    fn parses_after_case_insensitive_mention_strip() {
        let text = strip_bot_mention("/d_1@MyBot", "mybot");
        let parsed = StartDownloadCommand::parse(&text).unwrap();
        assert_eq!(parsed.id, 1);
    }

    #[test]
    fn rejects_non_numeric_book_id() {
        assert!(StartDownloadCommand::parse("/d_abc").is_err());
    }

    #[test]
    fn round_trip_archive_sequence() {
        let cmd = DownloadArchiveCommand::Sequence { id: 3 };
        match DownloadArchiveCommand::parse(&cmd.to_string()).unwrap() {
            DownloadArchiveCommand::Sequence { id } => assert_eq!(id, 3),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_archive_author() {
        let cmd = DownloadArchiveCommand::Author { id: 4 };
        match DownloadArchiveCommand::parse(&cmd.to_string()).unwrap() {
            DownloadArchiveCommand::Author { id } => assert_eq!(id, 4),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_archive_translator() {
        let cmd = DownloadArchiveCommand::Translator { id: 6 };
        match DownloadArchiveCommand::parse(&cmd.to_string()).unwrap() {
            DownloadArchiveCommand::Translator { id } => assert_eq!(id, 6),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rejects_foreign_archive_prefix() {
        assert!(DownloadArchiveCommand::parse("/da_x_5").is_err());
    }
}
```

Append to `book_bot/src/bots/approved_bot/modules/book/commands.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::BookCommand;

    #[test]
    fn parses_author() {
        match BookCommand::parse("/a_5").unwrap() {
            BookCommand::Author { id } => assert_eq!(id, 5),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_translator() {
        match BookCommand::parse("/t_7").unwrap() {
            BookCommand::Translator { id } => assert_eq!(id, 7),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_sequence() {
        match BookCommand::parse("/s_9").unwrap() {
            BookCommand::Sequence { id } => assert_eq!(id, 9),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rejects_foreign_prefix() {
        assert!(BookCommand::parse("/x_5").is_err());
    }

    #[test]
    fn rejects_non_numeric_id() {
        assert!(BookCommand::parse("/a_abc").is_err());
    }
}
```

- [ ] **Step 7: Run the full test suite**

Run: `cargo test -p book_bot` (from `/Users/kurbezz/Projects/books_project/book_bot`)
Expected: all tests pass, no compile warnings about unused `bot_name` params.

- [ ] **Step 8: Commit**

```bash
git add book_bot/src/bots/approved_bot/modules/utils/filter_command.rs \
        book_bot/src/bots/approved_bot/modules/annotations/commands.rs \
        book_bot/src/bots/approved_bot/modules/download/commands.rs \
        book_bot/src/bots/approved_bot/modules/book/commands.rs
git commit -m "fix: centralize case-insensitive @botname stripping in filter_command"
```

---

### Task 2: Fix the `[a-zA-z]` language-code regex typo

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/settings/callback_data.rs`

**Interfaces:**
- No new interfaces — internal regex fix plus tests on existing `SettingsCallbackData`.

- [ ] **Step 1: Write the failing tests**

Append to `book_bot/src/bots/approved_bot/modules/settings/callback_data.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::SettingsCallbackData;
    use std::str::FromStr;

    #[test]
    fn round_trip_settings_menu() {
        match SettingsCallbackData::from_str(&SettingsCallbackData::Settings.to_string()).unwrap()
        {
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
```

- [ ] **Step 2: Run the tests to verify `rejects_garbage_language_code` fails**

Run: `cargo test -p book_bot --package book_bot settings::callback_data`

Actual command (module path filter): `cargo test -p book_bot rejects_garbage_language_code`
Expected: FAIL — `SettingsCallbackData::from_str("lang_on__")` currently returns `Ok(..)` because `[a-zA-z]` matches the underscore, so `.is_err()` is false.

- [ ] **Step 3: Fix the regex**

In `book_bot/src/bots/approved_bot/modules/settings/callback_data.rs:8`, change:

```rust
static RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^lang_(?P<action>(off)|(on))_(?P<code>[a-zA-z]+)$").unwrap());
```

to:

```rust
static RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^lang_(?P<action>(off)|(on))_(?P<code>[a-zA-Z]+)$").unwrap());
```

- [ ] **Step 4: Run the tests to verify they all pass**

Run: `cargo test -p book_bot settings`
Expected: all `settings::callback_data::tests::*` pass (13 tests).

- [ ] **Step 5: Commit**

```bash
git add book_bot/src/bots/approved_bot/modules/settings/callback_data.rs
git commit -m "fix: correct [a-zA-z] typo in settings language-code regex"
```

---

### Task 3: Fix `RandomCallbackData` Display/FromStr asymmetry

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/random/callback_data.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/random/mod.rs:176-188,231-241`

**Interfaces:**
- `RandomCallbackData::to_string()` becomes self-sufficient: it serializes field data, so callers no longer need the `format!("{}_{index}", ...)` workaround.

- [ ] **Step 1: Write the failing tests**

Append to `book_bot/src/bots/approved_bot/modules/random/callback_data.rs`:

```rust
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
        match RandomCallbackData::from_str(&RandomCallbackData::RandomAuthor.to_string()).unwrap()
        {
            RandomCallbackData::RandomAuthor => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_random_sequence() {
        match RandomCallbackData::from_str(&RandomCallbackData::RandomSequence.to_string())
            .unwrap()
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
        assert_eq!(RandomCallbackData::Genres { index: 3 }.to_string(), "genres_3");
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
```

- [ ] **Step 2: Run the tests to verify `genres_display_includes_index` and `random_book_by_genre_display_includes_id` fail**

Run: `cargo test -p book_bot genres_display_includes_index random_book_by_genre_display_includes_id`
Expected: FAIL — today `RandomCallbackData::Genres { index: 3 }.to_string()` is `"genres"` (strum's derived `Display` ignores field data), not `"genres_3"`.

- [ ] **Step 3: Replace the derived `Display` with a manual impl and rewrite `FromStr`**

Replace the full contents of `book_bot/src/bots/approved_bot/modules/random/callback_data.rs` above the `#[cfg(test)]` block with:

```rust
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
```

This drops the now-unused `strum_macros::{Display, EnumIter}` import and the `EnumIter` derive (nothing outside this file iterates `RandomCallbackData` variants — confirmed via `grep -rn "RandomCallbackData" book_bot/src`).

- [ ] **Step 4: Remove the manual concatenation workaround in `random/mod.rs`**

In `book_bot/src/bots/approved_bot/modules/random/mod.rs`, in `get_genre_metas_handler`, replace:

```rust
                vec![InlineKeyboardButton {
                    kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(format!(
                        "{}_{index}",
                        RandomCallbackData::Genres {
                            index: index as u32
                        }
                    )),
                    text: genre_meta,
                }]
```

with:

```rust
                vec![InlineKeyboardButton {
                    kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                        RandomCallbackData::Genres {
                            index: index as u32,
                        }
                        .to_string(),
                    ),
                    text: genre_meta,
                }]
```

In `get_genres_by_meta_handler`, replace:

```rust
            vec![InlineKeyboardButton {
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(format!(
                    "{}_{}",
                    RandomCallbackData::RandomBookByGenre { id: genre.id },
                    genre.id
                )),
                text: genre.description,
            }]
```

with:

```rust
            vec![InlineKeyboardButton {
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    RandomCallbackData::RandomBookByGenre { id: genre.id }.to_string(),
                ),
                text: genre.description,
            }]
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p book_bot random`
Expected: all `random::callback_data::tests::*` pass (10 tests), and the crate still builds (`cargo build -p book_bot`).

- [ ] **Step 6: Commit**

```bash
git add book_bot/src/bots/approved_bot/modules/random/callback_data.rs \
        book_bot/src/bots/approved_bot/modules/random/mod.rs
git commit -m "fix: make RandomCallbackData Display/FromStr symmetric"
```

---

### Task 4: Normalize `page=0` in `UpdateLogCallbackData` and add missing tests to `search/callback_data.rs`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/update_history/callback_data.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/search/callback_data.rs`

**Interfaces:**
- No new interfaces — `UpdateLogCallbackData::from_str` now clamps `page` to a minimum of 1, matching the pattern already used in `search/callback_data.rs:46`, `annotations/callback_data.rs:27-32`, and `book/callback_data.rs:28-33`.

- [ ] **Step 1: Write the failing test for `UpdateLogCallbackData`**

Append to `book_bot/src/bots/approved_bot/modules/update_history/callback_data.rs`:

```rust
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
        let parsed =
            UpdateLogCallbackData::from_str("update_log_2024-01-01_2024-01-31_0").unwrap();
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
```

- [ ] **Step 2: Run the tests to verify `page_zero_normalized_to_one` fails**

Run: `cargo test -p book_bot page_zero_normalized_to_one`
Expected: the `update_history` variant of this test FAILS with `assertion 'left == right' failed \n left: 0 \n right: 1` (the `book`/`annotations` variants of the same test name already pass).

- [ ] **Step 3: Clamp `page` to a minimum of 1**

In `book_bot/src/bots/approved_bot/modules/update_history/callback_data.rs`, change:

```rust
        let page: u32 = caps["page"]
            .parse()
            .map_err(|_| strum::ParseError::VariantNotFound)?;

        Ok(UpdateLogCallbackData { from, to, page })
```

to:

```rust
        let page: u32 = caps["page"]
            .parse()
            .map_err(|_| strum::ParseError::VariantNotFound)?;
        let page: u32 = std::cmp::max(1, page);

        Ok(UpdateLogCallbackData { from, to, page })
```

- [ ] **Step 4: Add the missing tests to `search/callback_data.rs`**

Append to `book_bot/src/bots/approved_bot/modules/search/callback_data.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::SearchCallbackData;
    use std::str::FromStr;

    #[test]
    fn round_trip_book() {
        let cd = SearchCallbackData::Book { page: 3 };
        match SearchCallbackData::from_str(&cd.to_string()).unwrap() {
            SearchCallbackData::Book { page } => assert_eq!(page, 3),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_authors() {
        let cd = SearchCallbackData::Authors { page: 4 };
        match SearchCallbackData::from_str(&cd.to_string()).unwrap() {
            SearchCallbackData::Authors { page } => assert_eq!(page, 4),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_sequences() {
        let cd = SearchCallbackData::Sequences { page: 5 };
        match SearchCallbackData::from_str(&cd.to_string()).unwrap() {
            SearchCallbackData::Sequences { page } => assert_eq!(page, 5),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_translators() {
        let cd = SearchCallbackData::Translators { page: 6 };
        match SearchCallbackData::from_str(&cd.to_string()).unwrap() {
            SearchCallbackData::Translators { page } => assert_eq!(page, 6),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn page_zero_normalized_to_one() {
        match SearchCallbackData::from_str("sb_0").unwrap() {
            SearchCallbackData::Book { page } => assert_eq!(page, 1),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rejects_foreign_prefix() {
        assert!(SearchCallbackData::from_str("sx_1").is_err());
    }

    #[test]
    fn rejects_non_numeric_page() {
        assert!(SearchCallbackData::from_str("sb_abc").is_err());
    }
}
```

- [ ] **Step 5: Run the tests to verify they all pass**

Run: `cargo test -p book_bot update_history` then `cargo test -p book_bot search::callback_data`
Expected: all pass (4 tests in `update_history::callback_data::tests`, 7 in `search::callback_data::tests`).

- [ ] **Step 6: Commit**

```bash
git add book_bot/src/bots/approved_bot/modules/update_history/callback_data.rs \
        book_bot/src/bots/approved_bot/modules/search/callback_data.rs
git commit -m "fix: normalize page=0 in UpdateLogCallbackData; add search callback_data tests"
```

---

### Task 5: Round-trip and negative-case coverage for the three already-correct modules

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/annotations/callback_data.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/book/callback_data.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/download/callback_data.rs`

**Interfaces:**
- No behavior changes — these three files already normalize `page=0` and have symmetric `Display`/`FromStr`. This task only adds the round-trip/negative tests the spec requires.

- [ ] **Step 1: Add tests to `annotations/callback_data.rs`**

Append inside the existing `mod tests` block in `book_bot/src/bots/approved_bot/modules/annotations/callback_data.rs` (after `normal_page_preserved`):

```rust
    #[test]
    fn round_trip_book() {
        let cd = AnnotationCallbackData::Book { id: 10, page: 2 };
        match AnnotationCallbackData::from_str(&cd.to_string()).unwrap() {
            AnnotationCallbackData::Book { id, page } => {
                assert_eq!(id, 10);
                assert_eq!(page, 2);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_author() {
        let cd = AnnotationCallbackData::Author { id: 11, page: 5 };
        match AnnotationCallbackData::from_str(&cd.to_string()).unwrap() {
            AnnotationCallbackData::Author { id, page } => {
                assert_eq!(id, 11);
                assert_eq!(page, 5);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rejects_foreign_prefix() {
        assert!(AnnotationCallbackData::from_str("x_an_5_1").is_err());
    }

    #[test]
    fn rejects_non_numeric_id() {
        assert!(AnnotationCallbackData::from_str("b_an_abc_1").is_err());
    }
```

- [ ] **Step 2: Add tests to `book/callback_data.rs`**

Append inside the existing `mod tests` block in `book_bot/src/bots/approved_bot/modules/book/callback_data.rs` (after `normal_page_preserved`):

```rust
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
```

- [ ] **Step 3: Add a `#[cfg(test)]` block to `download/callback_data.rs`**

Append to `book_bot/src/bots/approved_bot/modules/download/callback_data.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::{CheckArchiveStatus, DownloadArchiveQueryData, DownloadQueryData};
    use std::str::FromStr;

    #[test]
    fn round_trip_download_data() {
        let cd = DownloadQueryData::DownloadData {
            book_id: 5,
            file_type: "fb2".to_string(),
        };
        match DownloadQueryData::from_str(&cd.to_string()).unwrap() {
            DownloadQueryData::DownloadData { book_id, file_type } => {
                assert_eq!(book_id, 5);
                assert_eq!(file_type, "fb2");
            }
        }
    }

    #[test]
    fn rejects_non_numeric_book_id() {
        assert!(DownloadQueryData::from_str("d_x_fb2").is_err());
    }

    #[test]
    fn round_trip_archive_sequence() {
        let cd = DownloadArchiveQueryData::Sequence {
            id: 3,
            file_type: "zip".to_string(),
        };
        match DownloadArchiveQueryData::from_str(&cd.to_string()).unwrap() {
            DownloadArchiveQueryData::Sequence { id, file_type } => {
                assert_eq!(id, 3);
                assert_eq!(file_type, "zip");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_archive_author() {
        let cd = DownloadArchiveQueryData::Author {
            id: 4,
            file_type: "fb2".to_string(),
        };
        match DownloadArchiveQueryData::from_str(&cd.to_string()).unwrap() {
            DownloadArchiveQueryData::Author { id, file_type } => {
                assert_eq!(id, 4);
                assert_eq!(file_type, "fb2");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_archive_translator() {
        let cd = DownloadArchiveQueryData::Translator {
            id: 6,
            file_type: "epub".to_string(),
        };
        match DownloadArchiveQueryData::from_str(&cd.to_string()).unwrap() {
            DownloadArchiveQueryData::Translator { id, file_type } => {
                assert_eq!(id, 6);
                assert_eq!(file_type, "epub");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rejects_foreign_archive_prefix() {
        assert!(DownloadArchiveQueryData::from_str("da_x_5_fb2").is_err());
    }

    #[test]
    fn round_trip_check_archive_status() {
        let cd = CheckArchiveStatus {
            task_id: "abc123".to_string(),
        };
        let parsed = CheckArchiveStatus::from_str(&cd.to_string()).unwrap();
        assert_eq!(parsed.task_id, "abc123");
    }

    #[test]
    fn rejects_check_archive_without_prefix() {
        assert!(CheckArchiveStatus::from_str("da_abc123").is_err());
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p book_bot annotations::callback_data` then `cargo test -p book_bot book::callback_data` then `cargo test -p book_bot download::callback_data`
Expected: all pass (7 tests in `annotations`, 7 in `book`, 7 in `download`).

- [ ] **Step 5: Run the entire suite once more**

Run: `cargo test -p book_bot` (from `/Users/kurbezz/Projects/books_project/book_bot`)
Expected: `test result: ok.` with 0 failures; total test count has grown from the baseline 63 by roughly 60 new tests across the 5 tasks.

- [ ] **Step 6: Commit**

```bash
git add book_bot/src/bots/approved_bot/modules/annotations/callback_data.rs \
        book_bot/src/bots/approved_bot/modules/book/callback_data.rs \
        book_bot/src/bots/approved_bot/modules/download/callback_data.rs
git commit -m "test: add round-trip and negative-case coverage for annotations/book/download callback data"
```
