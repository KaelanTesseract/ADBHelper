//! Texts in the user's language from Fluent files.
//!
//! English and German are built in (`locales/en.ftl`, `locales/de.ftl`). More
//! languages need no code change: a file such as `fr.ftl` in
//! `%APPDATA%\ADBHelper\languages` appears in Settings → Language. A text that
//! is missing in the selected language falls back to English, then to its id.

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use unic_langid::LanguageIdentifier;

const ENGLISH_SOURCE: &str = include_str!("../locales/en.ftl");
const GERMAN_SOURCE: &str = include_str!("../locales/de.ftl");
const ENGLISH: usize = 0;
const LANGUAGE_FOLDER: &str = "languages";

struct LanguageData {
    code: String,
    name: String,
    bundle: FluentBundle<FluentResource>,
}

fn build(code: &str, source: &str) -> Result<LanguageData, String> {
    let id: LanguageIdentifier = code
        .parse()
        .map_err(|_| format!("\"{code}\" is not a valid language code"))?;
    let resource = FluentResource::try_new(source.to_owned())
        .map_err(|(_, errors)| format!("{} syntax error(s) in {code}.ftl", errors.len()))?;
    let mut bundle = FluentBundle::new_concurrent(vec![id]);
    // Isolation marks would add invisible characters around every value.
    bundle.set_use_isolating(false);
    bundle
        .add_resource(resource)
        .map_err(|errors| format!("{} duplicate entries in {code}.ftl", errors.len()))?;
    let name = format(&bundle, "language-name", None)
        .ok_or_else(|| format!("{code}.ftl has no \"language-name\" entry"))?;
    Ok(LanguageData {
        code: code.to_owned(),
        name,
        bundle,
    })
}

fn format(
    bundle: &FluentBundle<FluentResource>,
    id: &str,
    args: Option<&FluentArgs>,
) -> Option<String> {
    let pattern = bundle.get_message(id)?.value()?;
    let mut errors = Vec::new();
    let value = bundle.format_pattern(pattern, args, &mut errors);
    errors.is_empty().then(|| value.into_owned())
}

/// Extra `<code>.ftl` files; broken files and built-in codes are skipped.
fn load_folder(folder: &Path) -> Vec<LanguageData> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "ftl"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let code = path.file_stem()?.to_str()?.replace('_', "-");
            if matches!(code.as_str(), "en" | "de") {
                return None;
            }
            let source = std::fs::read_to_string(&path).ok()?;
            build(&code, &source).ok()
        })
        .collect()
}

static LANGUAGES: OnceLock<Vec<LanguageData>> = OnceLock::new();
static CURRENT: AtomicUsize = AtomicUsize::new(ENGLISH);
// Plain texts are formatted once per language and kept for the app's lifetime,
// so `tr!` can hand out `&'static str` like the literals it replaced.
static PLAIN: OnceLock<Mutex<HashMap<(usize, &'static str), &'static str>>> = OnceLock::new();

fn languages() -> &'static [LanguageData] {
    LANGUAGES.get_or_init(|| {
        let mut all = vec![
            build("en", ENGLISH_SOURCE).expect("built-in English texts"),
            build("de", GERMAN_SOURCE).expect("built-in German texts"),
        ];
        if !cfg!(test) {
            if let Ok(folder) = crate::settings::file(LANGUAGE_FOLDER) {
                all.extend(load_folder(&folder));
            }
        }
        all
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Language(usize);

impl Language {
    pub fn current() -> Self {
        Self(CURRENT.load(Ordering::Relaxed))
    }

    /// Built-in languages first, then files from the language folder by name.
    pub fn all() -> Vec<Self> {
        let mut all: Vec<_> = (0..languages().len()).map(Self).collect();
        all[2..].sort_by_key(|language| language.name().to_lowercase());
        all
    }

    pub fn name(self) -> &'static str {
        &languages()[self.0].name
    }

    fn code(self) -> &'static str {
        &languages()[self.0].code
    }

    fn from_code(code: &str) -> Self {
        let code = code.trim();
        languages()
            .iter()
            .position(|language| language.code.eq_ignore_ascii_case(code))
            .map_or(Self(ENGLISH), Self)
    }

    fn activate(self) {
        CURRENT.store(self.0, Ordering::Relaxed);
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

fn resolve(id: &str, args: Option<&FluentArgs>) -> String {
    let all = languages();
    let current = Language::current().0;
    format(&all[current].bundle, id, args)
        .or_else(|| format(&all[ENGLISH].bundle, id, args))
        .unwrap_or_else(|| id.to_owned())
}

/// A text without placeholders in the current language. Use through `tr!`.
pub fn plain(id: &'static str) -> &'static str {
    let key = (Language::current().0, id);
    let cache = PLAIN.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .entry(key)
        .or_insert_with(|| Box::leak(resolve(id, None).into_boxed_str()))
}

/// A text with placeholders in the current language. Use through `tr!`.
pub fn with_args(id: &str, values: &[(&str, String)]) -> String {
    let mut args = FluentArgs::new();
    for (name, value) in values {
        args.set(*name, FluentValue::from(value.as_str()));
    }
    resolve(id, Some(&args))
}

/// `tr!("id")` gives `&'static str`; `tr!("id", name = value, …)` gives `String`.
/// Values are shown with `Display`; format them first when they need a precision.
macro_rules! tr {
    ($id:literal) => {
        $crate::localization::plain($id)
    };
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::localization::with_args($id, &[$((stringify!($name), ($value).to_string())),+])
    };
}

pub(crate) use tr;

#[cfg(test)]
mod tests;
