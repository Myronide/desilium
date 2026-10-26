//! Locale candidate chains used to choose localized desktop entry keys.

/// Locales that never carry translations and are therefore skipped.
const NEUTRAL_LOCALES: [&str; 2] = ["C", "POSIX"];

/// Expands one locale such as `de_DE.UTF-8@euro` into the lookup forms the spec tries,
/// most specific first. The encoding is dropped because desktop files never use it.
pub fn expand_locale(locale: &str) -> Vec<String> {
    let (head, modifier) = match locale.split_once('@') {
        Some((head, modifier)) => (head, Some(modifier)),
        None => (locale, None),
    };
    let head = head.split('.').next().unwrap_or("");
    let (lang, country) = match head.split_once('_') {
        Some((lang, country)) => (lang, Some(country)),
        None => (head, None),
    };
    if lang.is_empty() || NEUTRAL_LOCALES.contains(&lang) {
        return Vec::new();
    }
    let mut forms = Vec::with_capacity(4);
    if let (Some(country), Some(modifier)) = (country, modifier) {
        forms.push(format!("{lang}_{country}@{modifier}"));
    }
    if let Some(country) = country {
        forms.push(format!("{lang}_{country}"));
    }
    if let Some(modifier) = modifier {
        forms.push(format!("{lang}@{modifier}"));
    }
    forms.push(lang.to_owned());
    forms
}

/// Builds the candidate chain from `LANGUAGE` (colon list), `LC_MESSAGES` and `LANG`,
/// in that order, without duplicates.
pub fn locale_chain(
    language: Option<&str>,
    lc_messages: Option<&str>,
    lang: Option<&str>,
) -> Vec<String> {
    let sources = language
        .into_iter()
        .flat_map(|list| list.split(':'))
        .chain(lc_messages)
        .chain(lang);
    let mut chain: Vec<String> = Vec::new();
    for locale in sources {
        for candidate in expand_locale(locale) {
            if !chain.contains(&candidate) {
                chain.push(candidate);
            }
        }
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expansion_table() {
        let table: &[(&str, &[&str])] = &[
            ("de_DE.UTF-8", &["de_DE", "de"]),
            ("de_DE@euro", &["de_DE@euro", "de_DE", "de@euro", "de"]),
            ("fr", &["fr"]),
            (
                "sr_RS.UTF-8@latin",
                &["sr_RS@latin", "sr_RS", "sr@latin", "sr"],
            ),
            ("C", &[]),
            ("POSIX", &[]),
            ("", &[]),
        ];
        for (input, expected) in table {
            assert_eq!(expand_locale(input), *expected, "input {input}");
        }
    }

    #[test]
    fn chain_orders_language_list_then_fallbacks_without_duplicates() {
        let chain = locale_chain(Some("de_AT:en"), Some("de_AT.UTF-8"), Some("de_DE.UTF-8"));
        assert_eq!(chain, vec!["de_AT", "de", "en", "de_DE"]);
    }

    #[test]
    fn chain_is_empty_for_c_locale() {
        assert!(locale_chain(None, Some("C"), Some("C.UTF-8")).is_empty());
    }
}
