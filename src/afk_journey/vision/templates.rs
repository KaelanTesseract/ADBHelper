//! Template images embedded in the executable, decoded once and cached.

use crate::afk_journey::error::ActionError;
use crate::localization::{format_text, text};
use image::GrayImage;
use std::sync::{Mutex, OnceLock};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/templates.rs"));
}

/// Template bytes embedded by `build.rs` for every name used in `src/`.
fn embedded_template(relative_path: &str) -> Result<&'static [u8], ActionError> {
    generated::embedded(relative_path)
        .ok_or_else(|| {
            format_text!(
                "Template is not embedded in this build: {relative_path}",
                "Die Vorlage fehlt in dieser Version: {relative_path}"
            )
        })
        .map_err(ActionError::from)
}

pub(in crate::afk_journey) fn load_template(relative_path: &str) -> Result<GrayImage, ActionError> {
    static CACHE: OnceLock<Mutex<std::collections::HashMap<String, GrayImage>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Some(image) = cache
        .lock()
        .map_err(|_| {
            text(
                "template cache is unavailable",
                "Der Vorlagenspeicher ist nicht verfügbar",
            )
            .to_owned()
        })?
        .get(relative_path)
        .cloned()
    {
        return Ok(image);
    }

    let image = image::load_from_memory(embedded_template(relative_path)?)
        .map_err(|error| {
            format_text!(
                "Could not decode template {relative_path}: {error}",
                "Die Vorlage {relative_path} konnte nicht gelesen werden: {error}"
            )
        })?
        .to_luma8();
    cache
        .lock()
        .map_err(|_| {
            text(
                "template cache is unavailable",
                "Der Vorlagenspeicher ist nicht verfügbar",
            )
            .to_owned()
        })?
        .insert(relative_path.to_owned(), image.clone());
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_template_decodes_and_unknown_names_fail_clearly() {
        assert!(generated::EMBEDDED.len() > 150);
        for path in generated::EMBEDDED {
            let image = load_template(path).unwrap_or_else(|error| panic!("{path}: {error}"));
            assert!(image.width() > 0 && image.height() > 0, "{path}");
        }
        assert!(generated::EMBEDDED.contains(&"equipment/warrior.png"));
        assert!(generated::EMBEDDED.contains(&"legend_trials/banner_wilder.png"));
        assert!(load_template("no_such_folder/missing.png").is_err());
    }
}
