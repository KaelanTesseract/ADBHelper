use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    English,
    German,
}

// The window, update watcher, and automation workers share one language.
static GERMAN: AtomicBool = AtomicBool::new(false);

impl Language {
    pub const ALL: [Self; 2] = [Self::English, Self::German];

    pub fn current() -> Self {
        if GERMAN.load(Ordering::Relaxed) {
            Self::German
        } else {
            Self::English
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::German => "Deutsch",
        }
    }

    fn from_code(code: &str) -> Self {
        match code.trim() {
            "de" => Self::German,
            _ => Self::default(),
        }
    }

    fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::German => "de",
        }
    }

    fn activate(self) {
        GERMAN.store(self == Self::German, Ordering::Relaxed);
    }

    pub fn load() {
        let language = crate::settings::file("language")
            .ok()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map_or_else(Self::default, |code| Self::from_code(&code));
        language.activate();
    }

    pub fn select(self) -> io::Result<()> {
        self.activate();
        crate::settings::write("language", self.code().as_bytes())
    }
}

pub fn text<'a>(english: &'a str, german: &'a str) -> &'a str {
    match Language::current() {
        Language::English => english,
        Language::German => german,
    }
}

macro_rules! format_text {
    ($english:literal, $german:literal $(, $argument:expr)* $(,)?) => {
        match $crate::localization::Language::current() {
            $crate::localization::Language::English => format!($english $(, $argument)*),
            $crate::localization::Language::German => format!($german $(, $argument)*),
        }
    };
}

pub(crate) use format_text;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_default_and_saved_language_codes_round_trip() {
        assert_eq!(Language::default(), Language::English);
        assert_eq!(Language::from_code("unknown"), Language::English);
        for language in Language::ALL {
            assert_eq!(Language::from_code(language.code()), language);
        }
    }
}
