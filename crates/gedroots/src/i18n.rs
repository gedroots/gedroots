//! Interface languages.
//!
//! English is the source language: every string shown in the interface is a key in
//! `crates/gedroots/locales/*.yml`, with its English text and its translations side by side.
//! See `docs/localization.md` for how to add a string or a language.

use std::ops::Deref;

/// Languages the interface is translated into, source language first.
pub const LANGUAGES: [&str; 2] = ["en", "fr"];

/// Picks the interface language once, at startup: `GEDROOTS_LANG` if set (handy for testing),
/// otherwise the first of the system's preferred languages that GEDRoots supports, otherwise
/// English. On macOS the per-app language chosen in System Settings is part of that list.
pub fn init() {
    // Our French strings for gpui-component's own widgets (calendar, text field menu…).
    use gpui_kit::component as gpui_component;
    rust_i18n::extend!(gpui_component);

    let forced = std::env::var("GEDROOTS_LANG").ok();
    let preferred = forced.into_iter().chain(sys_locale::get_locales());
    let language = choose(preferred);
    rust_i18n::set_locale(language);
}

/// The first supported language among `preferred` (BCP 47 tags such as `fr-FR`, or POSIX ones
/// such as `fr_FR.UTF-8`), English if none is.
fn choose(preferred: impl IntoIterator<Item = String>) -> &'static str {
    preferred
        .into_iter()
        .find_map(|tag| {
            let primary = tag.split(['-', '_', '.', '@']).next()?.to_ascii_lowercase();
            LANGUAGES.into_iter().find(|l| *l == primary)
        })
        .unwrap_or(LANGUAGES[0])
}

/// The current interface language, e.g. `"fr"`.
#[allow(dead_code)]
pub fn language() -> impl Deref<Target = str> {
    rust_i18n::locale()
}

#[cfg(test)]
mod tests {
    use super::{LANGUAGES, choose};
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn picks_the_first_supported_language() {
        assert_eq!(choose(tags(&["fr-FR"])), "fr");
        assert_eq!(choose(tags(&["fr_CA.UTF-8"])), "fr");
        assert_eq!(choose(tags(&["de-DE", "fr-FR", "en-US"])), "fr");
        assert_eq!(choose(tags(&["en-GB", "fr-FR"])), "en");
        assert_eq!(choose(tags(&["de-DE"])), "en");
        assert_eq!(choose(tags(&[])), "en");
    }

    /// Every translated key of the app, per language (gpui-component's are left out: we only
    /// add French to them).
    fn catalog() -> BTreeMap<String, BTreeMap<String, String>> {
        let backend = crate::_rust_i18n_backend();
        let mut out: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for lang in LANGUAGES {
            for (key, text) in backend.messages_for_locale(lang).unwrap_or_default() {
                if key.starts_with("gpui_component.") {
                    continue;
                }
                out.entry(key.into_owned()).or_default().insert(lang.to_string(), text.into_owned());
            }
        }
        out
    }

    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.split("%{").skip(1).filter_map(|s| s.split_once('}')).map(|(name, _)| name).collect()
    }

    /// Each string exists in every language, with the same `%{…}` placeholders.
    #[test]
    fn every_string_is_translated() {
        let catalog = catalog();
        assert!(!catalog.is_empty());
        for (key, texts) in &catalog {
            for lang in LANGUAGES {
                let Some(text) = texts.get(lang) else { panic!("« {key} » has no {lang} text") };
                assert!(!text.trim().is_empty(), "« {key} » is empty in {lang}");
                assert_eq!(placeholders(text), placeholders(&texts["en"]), "« {key} »: placeholders differ in {lang}");
            }
        }
    }

    /// Every key the code asks for with the `t!` macro exists in the catalog, and every key in the
    /// catalog is still used somewhere (keys built at run time are listed in `DYNAMIC`).
    #[test]
    fn code_and_catalog_agree() {
        const DYNAMIC: &[&str] = &[];
        let catalog = catalog();
        let mut used = BTreeSet::new();
        let mut stack = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let source = std::fs::read_to_string(&path).unwrap();
                    let mut rest = source.as_str();
                    while let Some(at) = rest.find("t!(\"") {
                        let before = rest[..at].chars().next_back();
                        let chunk = &rest[at + 4..];
                        rest = chunk;
                        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                            continue; // another macro, such as `shortcut!`
                        }
                        let key = chunk.split('"').next().unwrap();
                        assert!(catalog.contains_key(key), "{}: « {key} » is not in locales/", path.display());
                        used.insert(key.to_string());
                    }
                }
            }
        }
        for key in catalog.keys() {
            let dynamic = DYNAMIC.iter().any(|prefix| key.starts_with(prefix));
            assert!(dynamic || used.contains(key), "« {key} » is in locales/ but no longer used");
        }
    }
}
