use super::*;
use fluent_syntax::ast;
use std::collections::{BTreeMap, BTreeSet};

fn parse(source: &str) -> ast::Resource<&str> {
    fluent_syntax::parser::parse(source).unwrap_or_else(|(_, errors)| panic!("{errors:?}"))
}

/// Placeholder names per message id.
fn placeholders(source: &str) -> BTreeMap<String, BTreeSet<String>> {
    fn collect(pattern: &ast::Pattern<&str>, found: &mut BTreeSet<String>) {
        for element in &pattern.elements {
            if let ast::PatternElement::Placeable { expression } = element {
                expression_vars(expression, found);
            }
        }
    }
    fn expression_vars(expression: &ast::Expression<&str>, found: &mut BTreeSet<String>) {
        match expression {
            ast::Expression::Inline(inline) => inline_vars(inline, found),
            ast::Expression::Select { selector, variants } => {
                inline_vars(selector, found);
                for variant in variants {
                    collect(&variant.value, found);
                }
            }
        }
    }
    fn inline_vars(inline: &ast::InlineExpression<&str>, found: &mut BTreeSet<String>) {
        match inline {
            ast::InlineExpression::VariableReference { id } => {
                found.insert(id.name.to_owned());
            }
            ast::InlineExpression::Placeable { expression } => expression_vars(expression, found),
            _ => {}
        }
    }
    parse(source)
        .body
        .iter()
        .filter_map(|entry| match entry {
            ast::Entry::Message(message) => {
                let mut found = BTreeSet::new();
                if let Some(value) = &message.value {
                    collect(value, &mut found);
                }
                Some((message.id.name.to_owned(), found))
            }
            _ => None,
        })
        .collect()
}

fn source_files(folder: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            source_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

#[test]
fn built_in_languages_load_with_their_names() {
    let all = Language::all();
    assert_eq!(all[0].name(), "English");
    assert_eq!(all[1].name(), "Deutsch");
    assert_eq!(Language::default(), all[0]);
    assert_eq!(Language::from_code("unknown"), all[0]);
    assert_eq!(Language::from_code(" DE "), all[1]);
    for language in all {
        assert_eq!(Language::from_code(language.code()), language);
    }
}

#[test]
fn english_and_german_have_the_same_ids_and_placeholders() {
    let english = placeholders(ENGLISH_SOURCE);
    let german = placeholders(GERMAN_SOURCE);
    let english_ids: BTreeSet<_> = english.keys().collect();
    let german_ids: BTreeSet<_> = german.keys().collect();
    assert_eq!(
        english_ids
            .symmetric_difference(&german_ids)
            .collect::<Vec<_>>(),
        Vec::<&&String>::new(),
        "ids missing in one language"
    );
    let different: Vec<_> = english
        .iter()
        .filter(|(id, vars)| german.get(*id) != Some(vars))
        .map(|(id, _)| id)
        .collect();
    assert!(different.is_empty(), "placeholders differ: {different:?}");
}

#[test]
fn every_id_used_in_the_code_exists() {
    let english = placeholders(ENGLISH_SOURCE);
    let mut files = Vec::new();
    source_files(
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
        &mut files,
    );
    let mut missing = BTreeSet::new();
    let mut used = 0;
    for file in files {
        // The localization module only mentions tr! in its documentation.
        if file.components().any(|part| {
            part.as_os_str()
                .to_string_lossy()
                .starts_with("localization")
        }) {
            continue;
        }
        let code = std::fs::read_to_string(&file).unwrap();
        for part in code.split("tr!(\"").skip(1) {
            let id = &part[..part.find('"').unwrap()];
            used += 1;
            if !english.contains_key(id) {
                missing.insert(format!("{}: {id}", file.display()));
            }
        }
    }
    assert!(used > 400, "only {used} tr! calls found");
    assert!(missing.is_empty(), "unknown ids: {missing:?}");
}

#[test]
fn english_texts_contain_no_german_characters() {
    let german_only: Vec<_> = ENGLISH_SOURCE
        .lines()
        .filter(|line| line.contains(['ä', 'ö', 'ü', 'ß', 'Ä', 'Ö', 'Ü', '„']))
        .collect();
    assert!(german_only.is_empty(), "{german_only:?}");
}

#[test]
fn texts_with_placeholders_fill_in_the_values() {
    let english = &languages()[0];
    let mut args = FluentArgs::new();
    args.set("error", FluentValue::from("boom"));
    let any_with_error = placeholders(ENGLISH_SOURCE)
        .into_iter()
        .find(|(_, vars)| vars.len() == 1 && vars.contains("error"))
        .map(|(id, _)| id)
        .expect("a text with an error placeholder");
    let text = format(&english.bundle, &any_with_error, Some(&args)).unwrap();
    assert!(text.contains("boom"), "{text}");
    assert!(!text.contains('\u{2068}'), "no isolation marks: {text:?}");
}

#[test]
fn extra_language_files_are_loaded_and_broken_ones_skipped() {
    let folder = std::env::temp_dir().join(format!("adbhelper-languages-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("fr.ftl"),
        "language-name = Français\nhello = Bonjour\n",
    )
    .unwrap();
    std::fs::write(folder.join("pt_BR.ftl"), "language-name = Português\n").unwrap();
    std::fs::write(folder.join("broken.ftl"), "language-name = {\n").unwrap();
    std::fs::write(folder.join("nl.ftl"), "hello = Hallo\n").unwrap();
    std::fs::write(folder.join("en.ftl"), "language-name = Replaced\n").unwrap();
    let loaded = load_folder(&folder);
    let codes: Vec<_> = loaded
        .iter()
        .map(|language| language.code.as_str())
        .collect();
    assert_eq!(codes, ["fr", "pt-BR"]);
    assert_eq!(loaded[0].name, "Français");
    assert_eq!(
        format(&loaded[0].bundle, "hello", None).as_deref(),
        Some("Bonjour")
    );
    std::fs::remove_dir_all(&folder).unwrap();
}

/// One-off check after moving the texts: every Fluent text must equal the old
/// Rust literal. Set ADBHELPER_L10N_BASELINE to the JSON the migration wrote.
#[test]
#[ignore = "needs the migration baseline"]
fn moved_texts_match_the_old_literals() {
    let path = std::env::var("ADBHELPER_L10N_BASELINE").expect("baseline path");
    let baseline: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut wrong = Vec::new();
    for entry in &baseline {
        let id = entry["id"].as_str().unwrap();
        let mut args = FluentArgs::new();
        for (name, value) in entry["args"].as_object().unwrap() {
            args.set(
                name.as_str(),
                FluentValue::from(value.as_str().unwrap().to_owned()),
            );
        }
        for (index, field) in [(0, "en"), (1, "de")] {
            let expected = entry[field].as_str().unwrap();
            let actual = format(&languages()[index].bundle, id, Some(&args));
            if actual.as_deref() != Some(expected) {
                wrong.push(format!("{field} {id}: {actual:?} != {expected:?}"));
            }
        }
    }
    assert!(baseline.len() > 500);
    assert!(
        wrong.is_empty(),
        "{} differences:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
