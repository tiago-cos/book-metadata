use chrono::{DateTime, NaiveDate, TimeZone, Utc};

const FULL_DATE_FORMATS: [&str; 6] = [
    "%Y-%m-%d",
    "%Y/%m/%d",
    "%B %d, %Y",
    "%b %d, %Y",
    "%d %B %Y",
    "%d %b %Y",
];

const MONTH_FORMATS: [&str; 3] = ["%d %B %Y", "%d %b %Y", "%Y-%m-%d"];

const LOOSE_DATE_FORMATS: [&str; 2] = ["%B %d %Y", "%b %d %Y"];

pub fn parse(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
        return Some(parsed.with_timezone(&Utc));
    }

    let cleaned = strip_ordinal_suffixes(raw);

    for format in FULL_DATE_FORMATS {
        if let Ok(date) = NaiveDate::parse_from_str(&cleaned, format) {
            return at_midnight(date);
        }
    }

    for format in MONTH_FORMATS {
        let candidate = if format.starts_with("%Y") {
            format!("{cleaned}-1")
        } else {
            format!("1 {cleaned}")
        };
        if let Ok(date) = NaiveDate::parse_from_str(&candidate, format) {
            return at_midnight(date);
        }
    }

    for format in LOOSE_DATE_FORMATS {
        if let Ok(date) = NaiveDate::parse_from_str(&cleaned, format) {
            return at_midnight(date);
        }
    }

    year_in(&cleaned).and_then(|year| NaiveDate::from_ymd_opt(year, 1, 1).and_then(at_midnight))
}

fn at_midnight(date: NaiveDate) -> Option<DateTime<Utc>> {
    Some(Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0)?))
}

fn strip_ordinal_suffixes(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    let mut previous_was_digit = false;

    while let Some(c) = chars.next() {
        if previous_was_digit && matches!(c, 's' | 'n' | 'r' | 't' | 'S' | 'N' | 'R' | 'T') {
            let suffix = match c.to_ascii_lowercase() {
                's' => 't',
                'n' | 'r' => 'd',
                _ => 'h',
            };
            if chars.peek().map(char::to_ascii_lowercase) == Some(suffix) {
                chars.next();
                previous_was_digit = false;
                continue;
            }
        }
        previous_was_digit = c.is_ascii_digit();
        out.push(c);
    }

    out
}

fn year_in(raw: &str) -> Option<i32> {
    let digits: Vec<char> = raw.chars().collect();
    digits.windows(4).find_map(|window| {
        if !window.iter().all(char::is_ascii_digit) {
            return None;
        }
        let year: i32 = window.iter().collect::<String>().parse().ok()?;
        (1000..=2999).contains(&year).then_some(year)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd(year: i32, month: u32, day: u32) -> Option<DateTime<Utc>> {
        Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).single()
    }

    #[test]
    fn parses_iso_dates() {
        assert_eq!(parse("2001-03-01"), ymd(2001, 3, 1));
        assert_eq!(parse("2001/03/01"), ymd(2001, 3, 1));
        assert_eq!(
            parse("2001-03-01T12:30:00Z"),
            ymd(2001, 3, 1)
                .map(|d| d + chrono::Duration::hours(12) + chrono::Duration::minutes(30))
        );
    }

    #[test]
    fn parses_written_out_dates() {
        assert_eq!(parse("September 1, 1988"), ymd(1988, 9, 1));
        assert_eq!(parse("Sep 1, 1988"), ymd(1988, 9, 1));
        assert_eq!(parse("1 September 1988"), ymd(1988, 9, 1));
        assert_eq!(parse("September 1st, 1988"), ymd(1988, 9, 1));
        assert_eq!(parse("December 3rd 1988"), ymd(1988, 12, 3));
        assert_eq!(parse("September 1 1988"), ymd(1988, 9, 1));
    }

    #[test]
    fn falls_back_to_the_first_of_the_month_or_year() {
        assert_eq!(parse("September 1988"), ymd(1988, 9, 1));
        assert_eq!(parse("Sep 1988"), ymd(1988, 9, 1));
        assert_eq!(parse("1988-09"), ymd(1988, 9, 1));
        assert_eq!(parse("1988"), ymd(1988, 1, 1));
    }

    #[test]
    fn digs_a_year_out_of_library_shorthand() {
        assert_eq!(parse("c1988"), ymd(1988, 1, 1));
        assert_eq!(parse("[1988]"), ymd(1988, 1, 1));
        assert_eq!(parse("1988?"), ymd(1988, 1, 1));
        assert_eq!(parse("published in 2004 by someone"), ymd(2004, 1, 1));
    }

    #[test]
    fn gives_up_on_anything_without_a_year() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("   "), None);
        assert_eq!(parse("sometime in the 90s"), None);
        assert_eq!(parse("unknown"), None);
    }
}
