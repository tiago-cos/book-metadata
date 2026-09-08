const SEPARATORS: [char; 6] = ['/', '&', '>', '|', ';', ','];

const SUBDIVISION: &str = "--";

pub fn normalize(tags: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut genres: Vec<String> = Vec::new();
    let mut keys: Vec<String> = Vec::new();

    for tag in tags {
        let tag = tag.replace(SUBDIVISION, "/");
        for fragment in tag.split(SEPARATORS) {
            let Some(genre) = tidy(fragment) else {
                continue;
            };
            let key = genre.to_lowercase();

            if let Some(index) = keys.iter().position(|seen| *seen == key) {
                if is_shouted(&genres[index]) && !is_shouted(&genre) {
                    genres[index] = genre;
                }
            } else {
                keys.push(key);
                genres.push(genre);
            }
        }
    }

    genres
}

fn tidy(fragment: &str) -> Option<String> {
    let collapsed = fragment.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .chars()
        .any(char::is_alphanumeric)
        .then_some(collapsed)
}

fn is_shouted(genre: &str) -> bool {
    genre.chars().any(char::is_alphabetic) && !genre.chars().any(char::is_lowercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalized(tags: &[&str]) -> Vec<String> {
        normalize(tags.iter().map(|t| (*t).to_owned()))
    }

    #[test]
    fn splits_packed_tags_and_folds_the_duplicates() {
        assert_eq!(
            normalized(&[
                "Fantasy",
                "Romance",
                "Fiction / Fantasy / Romance",
                "Science Fiction & Fantasy",
                "Adventure",
                "Fiction / Romance / Fantasy",
                "Young Adult",
            ]),
            vec![
                "Fantasy",
                "Romance",
                "Fiction",
                "Science Fiction",
                "Adventure",
                "Young Adult",
            ]
        );
    }

    #[test]
    fn handles_every_separator() {
        assert_eq!(
            normalized(&[
                "Fiction > Fantasy",
                "Horror; Thriller",
                "Crime, Mystery | Noir"
            ]),
            vec![
                "Fiction", "Fantasy", "Horror", "Thriller", "Crime", "Mystery", "Noir"
            ]
        );
    }

    #[test]
    fn folds_case_and_whitespace_differences() {
        assert_eq!(
            normalized(&["Science  Fiction", "SCIENCE FICTION", "science fiction"]),
            vec!["Science Fiction"]
        );
    }

    #[test]
    fn prefers_a_mixed_case_spelling_over_a_shouted_one() {
        assert_eq!(normalized(&["FICTION", "Fiction"]), vec!["Fiction"]);
        assert_eq!(normalized(&["Fiction", "FICTION"]), vec!["Fiction"]);
    }

    #[test]
    fn leaves_tags_that_are_only_ever_shouted_alone() {
        assert_eq!(normalized(&["LGBTQ+", "YA"]), vec!["LGBTQ+", "YA"]);
    }

    #[test]
    fn keeps_punctuation_that_belongs_to_the_genre() {
        assert_eq!(
            normalized(&["Sci-Fi", "Choose Your Own Adventure"]),
            vec!["Sci-Fi", "Choose Your Own Adventure"]
        );
    }

    #[test]
    fn splits_marc_subject_subdivisions() {
        assert_eq!(
            normalized(&["Foxes -- Fiction", "Fiction--Fantasy--Epic"]),
            vec!["Foxes", "Fiction", "Fantasy", "Epic"]
        );
        assert_eq!(normalized(&["Sci-Fi"]), vec!["Sci-Fi"]);
    }

    #[test]
    fn drops_empty_and_punctuation_only_fragments() {
        assert_eq!(
            normalized(&["Fantasy /", "/ / /", "  ", "-"]),
            vec!["Fantasy"]
        );
    }

    #[test]
    fn preserves_the_input_order() {
        assert_eq!(
            normalized(&["Zebras", "Antelopes"]),
            vec!["Zebras", "Antelopes"]
        );
    }
}
