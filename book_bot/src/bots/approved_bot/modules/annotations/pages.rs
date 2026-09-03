//! Split tokenized annotation markup into Telegram-message-sized pages,
//! producing both the rendered HTML and a plain-text counterpart (used for
//! length budgeting and for comparing against `Message::text()`, which is
//! entity-stripped).

use super::markup::{self, Tag, Token};

pub struct Page {
    pub html: String,
    pub plain: String,
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// Compute the UTF-16 length of the plain-text rendering of `raw`, without
/// building any HTML or paginating. Used to decide whether an annotation
/// fits in a single normal Telegram message before doing any heavier work.
pub fn plain_text_len(raw: &str) -> usize {
    let tokens = markup::tokenize(raw);
    tokens
        .iter()
        .map(|t| match t {
            Token::Text(s) => utf16_len(s),
            _ => 0,
        })
        .sum()
}

/// Render the *entire* tokenized input as a single, unpaginated HTML string
/// (plus its plain-text counterpart), with no budget/page-splitting.
///
/// This mirrors the exact same tag-stack/lazy-open rendering logic used by
/// [`build_pages`] (open tags are only emitted once real text follows them,
/// so e.g. `[b][/b]` with nothing in between renders as nothing at all,
/// matching per-page behavior) -- just without ever flushing to a new page.
///
/// Intended for the Telegram "Rich Message" path, where there is no
/// per-message size limit to budget against.
pub fn render_full(raw: &str) -> (String, String) {
    let tokens = markup::tokenize(raw);

    let mut stack: Vec<Tag> = Vec::new();
    let mut pending_opens: Vec<Tag> = Vec::new();
    let mut html = String::new();
    let mut plain = String::new();

    for token in tokens {
        match token {
            Token::Open(tag) => {
                stack.push(tag.clone());
                pending_opens.push(tag);
            }
            Token::Close(tag) => {
                stack.pop();
                if let Some(pos) = pending_opens.iter().rposition(|t| t == &tag) {
                    pending_opens.remove(pos);
                } else {
                    html.push_str(&markup::render_close(&tag));
                }
            }
            Token::Text(s) => {
                if !s.is_empty() {
                    for tag in pending_opens.drain(..) {
                        html.push_str(&markup::render_open(&tag));
                    }
                    html.push_str(&markup::escape_text(&s));
                    plain.push_str(&s);
                }
            }
        }
    }

    for tag in stack.iter().rev() {
        if !pending_opens.iter().any(|t| t == tag) {
            html.push_str(&markup::render_close(tag));
        }
    }

    (html, plain)
}

/// Find the byte index within `s` up to (and including) at most `room`
/// UTF-16 code units, preferring to cut at the last whitespace boundary
/// (word-wrap style). If no whitespace boundary exists, hard-cuts at
/// exactly `room` UTF-16 units (on a char boundary).
///
/// Returns `(cut_byte_idx, is_whitespace_boundary)`.
fn find_cut_point(s: &str, room: usize) -> usize {
    if room == 0 {
        return 0;
    }

    let mut units = 0usize;
    let mut last_ws_byte_idx: Option<usize> = None;
    let mut hard_cut_byte_idx = s.len();
    let mut found_hard_cut = false;

    for (byte_idx, ch) in s.char_indices() {
        let ch_units = ch.len_utf16();
        if units + ch_units > room {
            hard_cut_byte_idx = byte_idx;
            found_hard_cut = true;
            break;
        }
        units += ch_units;
        if ch.is_whitespace() {
            last_ws_byte_idx = Some(byte_idx + ch.len_utf8());
        }
    }

    if !found_hard_cut {
        // whole string fits
        return s.len();
    }

    match last_ws_byte_idx {
        Some(idx) if idx > 0 && idx <= hard_cut_byte_idx => idx,
        _ => hard_cut_byte_idx,
    }
}

pub fn build_pages(raw: &str, budget: usize) -> Vec<Page> {
    // Guard against a zero budget, which would otherwise make no forward
    // progress possible (every chunk of text would need an empty page).
    let budget = budget.max(1);

    let tokens = markup::tokenize(raw);

    let mut pages: Vec<Page> = Vec::new();
    let mut stack: Vec<Tag> = Vec::new();
    let mut pending_opens: Vec<Tag> = Vec::new();
    let mut cur_html = String::new();
    let mut cur_plain = String::new();

    macro_rules! flush_page {
        () => {{
            // Close every tag currently on the stack (LIFO), only the ones
            // actually emitted matter -- but since pending_opens tags were
            // never emitted, we must not close them.
            for tag in stack.iter().rev() {
                if !pending_opens.iter().any(|t| t == tag) {
                    cur_html.push_str(&markup::render_close(tag));
                }
            }
            pages.push(Page {
                html: std::mem::take(&mut cur_html),
                plain: std::mem::take(&mut cur_plain),
            });
            // Reseed pending_opens from the current stack so the next page
            // reopens exactly what was open at the cut point.
            pending_opens = stack.clone();
        }};
    }

    for token in tokens {
        match token {
            Token::Open(tag) => {
                stack.push(tag.clone());
                pending_opens.push(tag);
            }
            Token::Close(tag) => {
                stack.pop();
                if let Some(pos) = pending_opens.iter().rposition(|t| t == &tag) {
                    // Never actually opened on the page: drop silently
                    // (handles [b][/b] -> renders nothing).
                    pending_opens.remove(pos);
                } else {
                    cur_html.push_str(&markup::render_close(&tag));
                }
            }
            Token::Text(mut s) => {
                loop {
                    let room = budget.saturating_sub(utf16_len(&cur_plain));
                    let s_units = utf16_len(&s);

                    if s_units <= room {
                        if !s.is_empty() {
                            for tag in pending_opens.drain(..) {
                                cur_html.push_str(&markup::render_open(&tag));
                            }
                            cur_html.push_str(&markup::escape_text(&s));
                            cur_plain.push_str(&s);
                        }
                        break;
                    }

                    if room == 0 {
                        // No room at all: force a page flush and retry with
                        // the full remaining budget on a fresh page.
                        flush_page!();
                        continue;
                    }

                    let cut_idx = find_cut_point(&s, room);
                    let (head, tail) = s.split_at(cut_idx);

                    if !head.is_empty() {
                        for tag in pending_opens.drain(..) {
                            cur_html.push_str(&markup::render_open(&tag));
                        }
                        cur_html.push_str(&markup::escape_text(head));
                        cur_plain.push_str(head);
                    }

                    flush_page!();

                    s = tail.trim_start().to_string();
                    if s.is_empty() {
                        break;
                    }
                }
            }
        }
    }

    if !cur_html.is_empty() || !cur_plain.is_empty() {
        for tag in stack.iter().rev() {
            if !pending_opens.iter().any(|t| t == tag) {
                cur_html.push_str(&markup::render_close(tag));
            }
        }
        pages.push(Page {
            html: cur_html,
            plain: cur_plain,
        });
    }

    // Whitespace-only (or otherwise fully invisible) input should produce
    // zero pages, matching `is_normal_text()` semantics upstream.
    if pages.iter().all(|p| p.plain.trim().is_empty()) {
        return Vec::new();
    }

    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16_len_of(s: &str) -> usize {
        s.chars().map(char::len_utf16).sum()
    }

    #[test]
    fn empty_input_produces_no_pages() {
        assert!(build_pages("", 100).is_empty());
        assert!(build_pages("   \n  ", 100).is_empty());
        assert!(build_pages("[b][/b]", 100).is_empty());
    }

    /// Regression test for a real-world pattern found in production data:
    /// an annotation with hundreds of `ammonia`-sanitized bookmark anchors
    /// (`<a rel="noopener noreferrer"></a>`, no href) interleaved with
    /// prose, spanning many pages. None of the empty anchors should leak
    /// as visible text, and every page's tags must stay balanced.
    #[test]
    fn large_input_with_many_empty_anchors_stays_clean_and_balanced() {
        let paragraph = "Строка текста для проверки. <a rel=\"noopener noreferrer\"></a>\n";
        let raw = paragraph.repeat(500);

        let pages = build_pages(&raw, 4096);
        assert!(pages.len() > 1, "expected input to span multiple pages");

        for (i, p) in pages.iter().enumerate() {
            assert!(
                utf16_len_of(&p.plain) <= 4096,
                "page {i} exceeds budget: {}",
                utf16_len_of(&p.plain)
            );
            assert!(
                !p.html.contains("<a rel="),
                "page {i} leaked raw '<a rel=' garbage: {:?}",
                &p.html[..p.html.len().min(200)]
            );
            assert!(
                !p.html.contains("noreferrer"),
                "page {i} leaked stripped rel attribute as text"
            );
            let opens = p.html.matches("<a href=").count();
            let closes = p.html.matches("</a>").count();
            assert_eq!(
                opens, closes,
                "page {i} unbalanced <a>: {opens} opens vs {closes} closes"
            );
        }
    }

    #[test]
    fn simple_text_single_page() {
        let pages = build_pages("hello world", 100);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].html, "hello world");
        assert_eq!(pages[0].plain, "hello world");
    }

    #[test]
    fn bold_renders_html() {
        let pages = build_pages("[b]hello[/b] world", 100);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].html, "<b>hello</b> world");
        assert_eq!(pages[0].plain, "hello world");
    }

    #[test]
    fn every_page_plain_within_budget() {
        let inputs = [
            "aaaaaaaaaa bbbbbbbbbb",
            "[b]bold text that is somewhat long and should wrap around multiple pages if the budget is small enough[/b]",
            "plain text without any markup at all that just keeps going and going and going",
        ];
        let budgets = [5usize, 10, 25, 50];

        for input in inputs.iter() {
            for &budget in budgets.iter() {
                let pages = build_pages(input, budget);
                for page in &pages {
                    assert!(
                        utf16_len_of(&page.plain) <= budget,
                        "page {:?} (utf16 len {}) exceeds budget {budget} for input {input:?}",
                        page.plain,
                        utf16_len_of(&page.plain)
                    );
                }
            }
        }
    }

    #[test]
    fn wrap_width_is_honored_not_hardcoded() {
        let input = "aaaaaaaaaa bbbbbbbbbb";
        let pages = build_pages(input, 10);
        for page in &pages {
            assert!(
                utf16_len_of(&page.plain) <= 10,
                "page {:?} (len {}) exceeds width 10",
                page.plain,
                utf16_len_of(&page.plain)
            );
        }
    }

    #[test]
    fn wrap_width_above_512_is_honored() {
        let word = "a".repeat(20);
        let input = std::iter::repeat_n(word, 50).collect::<Vec<_>>().join(" ");
        let width = 1000;
        let pages = build_pages(&input, width);
        for page in &pages {
            assert!(
                utf16_len_of(&page.plain) <= width,
                "page {:?} (len {}) exceeds width {width}",
                page.plain,
                utf16_len_of(&page.plain)
            );
        }
    }

    #[test]
    fn plain_text_round_trips_and_wraps_at_word_boundaries() {
        let input = "\n Библиотека современной фантастики. Том 21\n Содержание:\n РОМАН И ПОВЕСТИ:\n Разбивая стеклянные двери… Предисловие В. Ревича\n Джон Бойнтон Пристли. Дженни Вильерс. Роман о театре. Перевод с английского В. Ашкенази\n Уильям Сароян. Тигр Тома Трейси. Повесть. Перевод с английского Р. Рыбкина\n Роберт Янг. Срубить дерево. Повесть. Перевод с английского С. Васильевой\n РАССКАЗЫ:\n Жан Рей. Рука Геца фон Берлихингена. Перевод с французского А. Григорьева\n Клод Легран. По мерке. Перевод с французского А. Григорьева\n Саке Комацу. Смерть Бикуни. Перевод с японского З. Рахима\n Ана Мария Матуте. Король Зеннов. Перевод с испанского Е. Любимовой\n Антонио Минготе. Николас. Перевод с испанского Р. Рыбкина\n Юн Бинг. Риестофер Юсеф. Перевод с норвежского Л. Жданова\n Гораций Голд. Чего стоят крылья. Перевод с английского Ф. Мендельсона\n Питер С. Бигл. Милости просим, леди Смерть! Перевод с английского Я. Евдокимовой\n Андре Майе. Как я стала писательницей. Перевод с французского Р. Рыбкина\n Джеймс Поллард. Заколдованный поезд. Перевод с английского Р. Рыбкина\n Рэй Брэдбери. Апрельское колдовство. Перевод с английского Л. Жданова\n Айзек Азимов. Небывальщина. Перевод с английского К. Сенина и В. Тальми\n Р.А. Лэфферти. Семь дней ужаса. Перевод с английского И. Почиталина\n Генри Каттнер. Сим удостоверяется… Перевод с английского К. Сенина и В. Тальми\n ";

        let pages = build_pages(input, 512);
        assert!(!pages.is_empty());

        for page in &pages {
            assert!(utf16_len_of(&page.plain) <= 512);
        }

        let joined_expected: String = input.split_whitespace().collect::<Vec<_>>().join(" ");
        let joined_actual: String = pages
            .iter()
            .map(|p| p.plain.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        assert_eq!(joined_actual, joined_expected);
    }

    #[test]
    fn markup_spanning_forced_page_boundary_stays_balanced() {
        let long_text = "word ".repeat(50);
        let input = format!("[b]{long_text}[/b]");
        let pages = build_pages(&input, 30);
        assert!(pages.len() > 1, "expected multiple pages to be produced");

        for page in &pages {
            let open_count = page.html.matches("<b>").count();
            let close_count = page.html.matches("</b>").count();
            assert_eq!(
                open_count, close_count,
                "unbalanced <b> tags in page html: {:?}",
                page.html
            );
        }
    }

    #[test]
    fn render_full_matches_single_page_for_short_inputs() {
        let inputs = [
            "hello world",
            "[b]hello[/b] world",
            "[b][i]x[/b]y[/i]",
            "[b][/b]",
            "before<a rel=\"noopener noreferrer\"></a>after",
            "<a href=\"http://x.com\" rel=\"noopener noreferrer\">l</a>",
        ];

        for input in inputs {
            let pages = build_pages(input, 4096);
            let (full_html, full_plain) = render_full(input);

            match pages.first() {
                Some(page) => {
                    assert_eq!(full_html, page.html, "html mismatch for {input:?}");
                    assert_eq!(full_plain, page.plain, "plain mismatch for {input:?}");
                }
                None => {
                    assert!(full_html.is_empty(), "expected empty html for {input:?}");
                    assert!(full_plain.is_empty(), "expected empty plain for {input:?}");
                }
            }
        }
    }

    #[test]
    fn render_full_large_input_with_many_empty_anchors_stays_clean_and_balanced() {
        let paragraph = "Строка текста для проверки. <a rel=\"noopener noreferrer\"></a>\n";
        let raw = paragraph.repeat(500);

        let (html, plain) = render_full(&raw);

        assert!(!plain.is_empty());
        assert!(
            !html.contains("<a rel="),
            "leaked raw '<a rel=' garbage: {:?}",
            &html[..html.len().min(200)]
        );
        assert!(
            !html.contains("noreferrer"),
            "leaked stripped rel attribute as text"
        );
        let opens = html.matches("<a href=").count();
        let closes = html.matches("</a>").count();
        assert_eq!(
            opens, closes,
            "unbalanced <a>: {opens} opens vs {closes} closes"
        );
    }

    #[test]
    fn small_budget_zero_edge_case_does_not_infinite_loop() {
        // budget smaller than any single character's UTF-16 width in
        // practice never happens (min is 1), but budget of 1 should still
        // terminate correctly.
        let pages = build_pages("hello world", 1);
        assert!(!pages.is_empty());
        for page in &pages {
            assert!(utf16_len_of(&page.plain) <= 1);
        }
    }
}
