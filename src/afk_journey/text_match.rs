//! Comparison keys for OCR text: case, spaces, and punctuation removed.
//!
//! OCR output varies in spacing, punctuation, and case between captures, so
//! game labels are compared through one of these keys instead of raw text.

/// Letters and digits of any script, lowercased. For player and guild names,
/// which may contain accents or non-Latin letters.
pub(super) fn letters_key(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// ASCII letters and digits, lowercased. For the game's English UI labels.
pub(super) fn ascii_key(value: &str) -> String {
    ascii_key_with(value, &[])
}

/// Like [`ascii_key`], but also keeps the listed characters, for labels whose
/// meaning depends on them, such as `2/3` counters or the `+` in `Elite+`.
pub(super) fn ascii_key_with(value: &str, keep: &[char]) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || keep.contains(c))
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_ignore_case_spaces_and_punctuation() {
        assert_eq!(ascii_key("Read All!"), "readall");
        assert_eq!(ascii_key("Noble Path II"), "noblepathii");
        assert_eq!(ascii_key_with("Chances: 2 / 3", &['/']), "chances2/3");
        assert_eq!(ascii_key_with("Elite +", &['+']), "elite+");
        assert_eq!(ascii_key_with("Elite +", &[]), "elite");
    }

    #[test]
    fn only_letters_key_keeps_non_ascii_letters() {
        assert_eq!(letters_key("Größe-Ä 7"), "größeä7");
        assert_eq!(ascii_key("Größe-Ä 7"), "gre7");
        assert_eq!(letters_key("Ångström"), "ångström");
    }
}
