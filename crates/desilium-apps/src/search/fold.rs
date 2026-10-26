//! Case- and diacritic-insensitive folding that keeps a map back to the original text.

use std::ops::Range;

/// Folded characters with the byte range each one came from in the original string.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Folded {
    /// One folded character per original character.
    pub chars: Vec<char>,
    /// `spans[i]` is the byte range in the original text of `chars[i]`.
    pub spans: Vec<Range<usize>>,
}

/// Folds `text` one character at a time, so positions in the folded form map back exactly.
pub fn fold(text: &str) -> Folded {
    let mut folded = Folded::default();
    for (start, c) in text.char_indices() {
        folded.chars.push(fold_char(c));
        folded.spans.push(start..start + c.len_utf8());
    }
    folded
}

/// Lowercases one character and strips common Latin diacritics.
pub fn fold_char(c: char) -> char {
    let lower = c.to_lowercase().next().unwrap_or(c);
    match lower {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'æ' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
        'ğ' => 'g',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ı' => 'i',
        'ł' | 'ľ' => 'l',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'œ' => 'o',
        'ř' => 'r',
        'ś' | 'š' | 'ş' | 'ß' => 's',
        'ť' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' => 'u',
        'ý' | 'ÿ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_case_and_diacritics() {
        let folded = fold("Éclair Straße");
        let text: String = folded.chars.iter().collect();
        assert_eq!(text, "eclair strase");
        assert_eq!(folded.chars.len(), "Éclair Straße".chars().count());
    }

    #[test]
    fn spans_point_back_into_the_original() {
        let original = "aé€";
        let folded = fold(original);
        assert_eq!(folded.spans[1], 1..3);
        assert_eq!(&original[folded.spans[1].clone()], "é");
        assert_eq!(&original[folded.spans[2].clone()], "€");
    }
}
