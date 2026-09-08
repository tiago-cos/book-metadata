pub const BOOK_FIELDS: &str = r"
fragment BookFields on books {
  id
  title
  subtitle
  description
  pages
  release_date
  cached_tags
  image { url }
  contributions {
    contribution
    author { name }
  }
  book_series {
    position
    details
    series { name }
  }
}
";

pub const EDITION_FIELDS: &str = r"
fragment EditionFields on editions {
  title
  subtitle
  isbn_10
  isbn_13
  pages
  release_date
  edition_format
  publisher { name }
  language { language code3 }
  image { url }
}
";

pub fn book_by_isbn() -> String {
    format!(
        r"{BOOK_FIELDS}{EDITION_FIELDS}
query BookByIsbn($isbn: String!, $limit: Int!) {{
  editions(
    where: {{ _or: [{{ isbn_10: {{ _eq: $isbn }} }}, {{ isbn_13: {{ _eq: $isbn }} }}] }}
    limit: $limit
  ) {{
    ...EditionFields
    book {{ ...BookFields }}
  }}
}}
"
    )
}

pub fn books_search() -> String {
    format!(
        r"{BOOK_FIELDS}{EDITION_FIELDS}
query BooksSearch($where: books_bool_exp!, $limit: Int!, $editionLimit: Int!) {{
  books(
    where: $where
    order_by: [{{ users_count: desc_nulls_last }}]
    limit: $limit
  ) {{
    ...BookFields
    editions(limit: $editionLimit, order_by: [{{ users_count: desc_nulls_last }}]) {{
      ...EditionFields
      users_count
    }}
  }}
}}
"
    )
}
