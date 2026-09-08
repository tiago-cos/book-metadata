const NAMES: [(&str, &str); 100] = [
    ("en", "English"),
    ("eng", "English"),
    ("es", "Spanish"),
    ("spa", "Spanish"),
    ("fr", "French"),
    ("fre", "French"),
    ("fra", "French"),
    ("de", "German"),
    ("ger", "German"),
    ("deu", "German"),
    ("it", "Italian"),
    ("ita", "Italian"),
    ("pt", "Portuguese"),
    ("por", "Portuguese"),
    ("nl", "Dutch"),
    ("dut", "Dutch"),
    ("nld", "Dutch"),
    ("ru", "Russian"),
    ("rus", "Russian"),
    ("ja", "Japanese"),
    ("jpn", "Japanese"),
    ("zh", "Chinese"),
    ("chi", "Chinese"),
    ("zho", "Chinese"),
    ("ko", "Korean"),
    ("kor", "Korean"),
    ("ar", "Arabic"),
    ("ara", "Arabic"),
    ("he", "Hebrew"),
    ("heb", "Hebrew"),
    ("hi", "Hindi"),
    ("hin", "Hindi"),
    ("sv", "Swedish"),
    ("swe", "Swedish"),
    ("no", "Norwegian"),
    ("nor", "Norwegian"),
    ("da", "Danish"),
    ("dan", "Danish"),
    ("fi", "Finnish"),
    ("fin", "Finnish"),
    ("pl", "Polish"),
    ("pol", "Polish"),
    ("tr", "Turkish"),
    ("tur", "Turkish"),
    ("el", "Greek"),
    ("gre", "Greek"),
    ("ell", "Greek"),
    ("cs", "Czech"),
    ("cze", "Czech"),
    ("ces", "Czech"),
    ("hu", "Hungarian"),
    ("hun", "Hungarian"),
    ("ro", "Romanian"),
    ("rum", "Romanian"),
    ("ron", "Romanian"),
    ("uk", "Ukrainian"),
    ("ukr", "Ukrainian"),
    ("vi", "Vietnamese"),
    ("vie", "Vietnamese"),
    ("th", "Thai"),
    ("tha", "Thai"),
    ("id", "Indonesian"),
    ("ind", "Indonesian"),
    ("ca", "Catalan"),
    ("cat", "Catalan"),
    ("gl", "Galician"),
    ("glg", "Galician"),
    ("eu", "Basque"),
    ("baq", "Basque"),
    ("eus", "Basque"),
    ("la", "Latin"),
    ("lat", "Latin"),
    ("bg", "Bulgarian"),
    ("bul", "Bulgarian"),
    ("hr", "Croatian"),
    ("hrv", "Croatian"),
    ("sr", "Serbian"),
    ("srp", "Serbian"),
    ("sk", "Slovak"),
    ("slo", "Slovak"),
    ("slk", "Slovak"),
    ("sl", "Slovenian"),
    ("slv", "Slovenian"),
    ("et", "Estonian"),
    ("est", "Estonian"),
    ("lv", "Latvian"),
    ("lav", "Latvian"),
    ("lt", "Lithuanian"),
    ("lit", "Lithuanian"),
    ("fa", "Persian"),
    ("per", "Persian"),
    ("fas", "Persian"),
    ("is", "Icelandic"),
    ("ice", "Icelandic"),
    ("isl", "Icelandic"),
    ("ga", "Irish"),
    ("gle", "Irish"),
    ("cy", "Welsh"),
    ("wel", "Welsh"),
    ("cym", "Welsh"),
];

pub fn name(code: &str) -> String {
    let code = code.trim();
    let base = code.split(['-', '_']).next().unwrap_or(code);
    let lowered = base.to_lowercase();

    NAMES
        .iter()
        .find(|(candidate, _)| *candidate == lowered)
        .map_or_else(|| code.to_owned(), |(_, name)| (*name).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_both_code_lengths() {
        assert_eq!(name("en"), "English");
        assert_eq!(name("eng"), "English");
        assert_eq!(name("pt"), "Portuguese");
        assert_eq!(name("por"), "Portuguese");
    }

    #[test]
    fn names_bibliographic_and_terminological_variants_alike() {
        assert_eq!(name("ger"), "German");
        assert_eq!(name("deu"), "German");
        assert_eq!(name("fre"), name("fra"));
    }

    #[test]
    fn ignores_casing_and_region_suffixes() {
        assert_eq!(name("EN"), "English");
        assert_eq!(name("en-GB"), "English");
        assert_eq!(name("pt-BR"), "Portuguese");
        assert_eq!(name("zh_CN"), "Chinese");
        assert_eq!(name("  eng  "), "English");
    }

    #[test]
    fn passes_through_what_it_does_not_know() {
        assert_eq!(name("tlh"), "tlh");
        assert_eq!(name("Klingon"), "Klingon");
        assert_eq!(name("English"), "English");
    }
}
