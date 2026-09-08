use crate::metadata::BookSeries;

const POSITION_KEYWORDS: [&str; 10] = [
    "book", "bk", "vol", "volume", "no", "num", "number", "part", "pt", "episode",
];

pub fn position_in_label(text: &str) -> Option<f32> {
    let mut current = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() || (c == '.' && !current.is_empty() && !current.contains('.')) {
            current.push(c);
        } else if !current.is_empty() {
            break;
        }
    }
    current.trim_end_matches('.').parse::<f32>().ok()
}

pub fn parse_combined(raw: &str) -> Option<BookSeries> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    if let Some(index) = raw.rfind([';', ',']) {
        let (head, tail) = raw.split_at(index);
        let head = head.trim_end_matches([';', ',']).trim();
        let tail = tail[1..].trim();
        if !head.is_empty() && looks_like_position(tail) {
            return Some(BookSeries {
                title: head.to_owned(),
                number: position_in_label(tail),
            });
        }
    }

    if let Some((title, number)) = split_trailing_position(raw) {
        return Some(BookSeries {
            title,
            number: Some(number),
        });
    }

    Some(BookSeries::unnumbered(raw))
}

fn looks_like_position(tail: &str) -> bool {
    tail.chars().any(|c| c.is_ascii_digit()) && tail.split_whitespace().count() <= 3
}

fn split_trailing_position(raw: &str) -> Option<(String, f32)> {
    let words: Vec<&str> = raw.split_whitespace().collect();
    let last = words.last()?;

    if let Some(number) = last.strip_prefix('#').and_then(position_in_label_exact) {
        let title = words[..words.len() - 1].join(" ");
        return (!title.is_empty()).then_some((title, number));
    }

    let number = position_in_label_exact(last)?;
    let keyword = words.get(words.len().checked_sub(2)?)?;
    let keyword = keyword.trim_end_matches('.').to_lowercase();
    if !POSITION_KEYWORDS.contains(&keyword.as_str()) {
        return None;
    }

    let title = words[..words.len() - 2].join(" ");
    (!title.is_empty()).then_some((title, number))
}

fn position_in_label_exact(word: &str) -> Option<f32> {
    word.parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combined(raw: &str) -> Option<BookSeries> {
        parse_combined(raw)
    }

    #[test]
    fn extracts_numbers_from_position_labels() {
        assert_eq!(position_in_label("#1"), Some(1.0));
        assert_eq!(position_in_label("Book 2.5"), Some(2.5));
        assert_eq!(position_in_label("Volume 3 of 7"), Some(3.0));
        assert_eq!(position_in_label("prequel"), None);
    }

    #[test]
    fn splits_a_position_off_a_separator() {
        assert_eq!(
            combined("Harry Potter ; 3"),
            Some(BookSeries::new("Harry Potter", 3.0))
        );
        assert_eq!(
            combined("The Chronicles of Narnia ; bk. 2"),
            Some(BookSeries::new("The Chronicles of Narnia", 2.0))
        );
        assert_eq!(
            combined("A Song of Ice and Fire, book 2"),
            Some(BookSeries::new("A Song of Ice and Fire", 2.0))
        );
        assert_eq!(
            combined("Lord of the Rings, The, part 1"),
            Some(BookSeries::new("Lord of the Rings, The", 1.0))
        );
    }

    #[test]
    fn splits_a_marked_position_off_the_end() {
        assert_eq!(
            combined("Discworld #3"),
            Some(BookSeries::new("Discworld", 3.0))
        );
        assert_eq!(
            combined("The Wheel of Time Book 5"),
            Some(BookSeries::new("The Wheel of Time", 5.0))
        );
    }

    #[test]
    fn keeps_a_name_that_merely_ends_in_a_number() {
        assert_eq!(
            combined("Fahrenheit 451"),
            Some(BookSeries::unnumbered("Fahrenheit 451"))
        );
        assert_eq!(
            combined("Discworld 3"),
            Some(BookSeries::unnumbered("Discworld 3"))
        );
    }

    #[test]
    fn keeps_a_comma_that_is_part_of_the_name() {
        assert_eq!(
            combined("Pride, Prejudice and Zombies"),
            Some(BookSeries::unnumbered("Pride, Prejudice and Zombies"))
        );
    }

    #[test]
    fn keeps_an_unnumbered_series() {
        assert_eq!(
            combined("Discworld"),
            Some(BookSeries::unnumbered("Discworld"))
        );
        assert_eq!(combined("  "), None);
    }
}
