//! SQL identifier validation and quoting.
//!
//! Two kinds of identifiers reach generated SQL:
//!
//! * **Declared** identifiers (table names, field names, `db_column`, foreign-key
//!   targets) come from model declarations. They are trusted to *mean* what the
//!   developer wrote, but they are still quoted with embedded `"` doubled, so a
//!   strange `db_column` can never break out of its quotes.
//! * **Query** identifiers (`filter` / `exclude` / `order_by` / `update` keys)
//!   may come from request data. They must pass [`is_identifier`] before they
//!   are even looked up; values are parameterized, identifiers cannot be.

/// `true` when `s` matches `^[A-Za-z_][A-Za-z0-9_]*$` over the whole string,
/// ASCII only (a trailing newline or a non-ASCII letter does not pass).
#[inline]
pub fn is_identifier(s: &str) -> bool {
    let bytes = s.as_bytes();
    match bytes.first() {
        Some(b) if b.is_ascii_alphabetic() || *b == b'_' => {}
        _ => return false,
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

/// Quote a SQL identifier, doubling any embedded double quotes.
pub fn quote_ident(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    out.push('"');
    for ch in name.chars() {
        if ch == '"' {
            out.push('"');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

/// Render a SQL string literal, doubling any embedded single quotes.
pub fn quote_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for ch in text.chars() {
        if ch == '\'' {
            out.push('\'');
        }
        out.push(ch);
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_rule() {
        for ok in ["a", "_", "_a1", "Name", "author_id", "A_B_9"] {
            assert!(is_identifier(ok), "{ok}");
        }
        for bad in [
            "",
            "1a",
            "a b",
            "a-b",
            "a\"",
            "name\n",
            "näme",
            "a;",
            "x\" = 1 OR \"1",
        ] {
            assert!(!is_identifier(bad), "{bad:?}");
        }
    }

    #[test]
    fn quoting() {
        assert_eq!(quote_ident("users"), "\"users\"");
        assert_eq!(quote_ident("a\"b"), "\"a\"\"b\"");
        assert_eq!(quote_literal("it's"), "'it''s'");
    }
}
