//! Language of the text this library writes: labels, dates, messages, and the titles it gives
//! to new records.
//!
//! The catalogs are in `crates/gedcom/locales/*.yml` (English and French side by side, see
//! `docs/localization.md`). The language is the one the app chose with `rust_i18n::set_locale`,
//! shared by every crate; English when nothing chose one, as in tests. [`with_language`] writes
//! in another one for a while, on the current thread only: a family book is written in the
//! tree's language, whatever the interface's.

use std::borrow::Cow;
use std::cell::RefCell;

/// Languages the library is translated into, source language first.
pub const LANGUAGES: [&str; 2] = ["en", "fr"];

thread_local! {
    /// The language [`with_language`] chose on this thread, if any.
    static SCOPED: RefCell<Option<&'static str>> = const { RefCell::new(None) };
}

/// The current language, e.g. `"fr"`.
pub fn language() -> Cow<'static, str> {
    match SCOPED.with(|s| *s.borrow()) {
        Some(l) => Cow::Borrowed(l),
        None => Cow::Owned(rust_i18n::locale().to_string()),
    }
}

/// Runs `f` with the text written in `lang` (one of [`LANGUAGES`]) on this thread.
pub fn with_language<R>(lang: &'static str, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<&'static str>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SCOPED.with(|s| *s.borrow_mut() = self.0);
        }
    }
    let _restore = Restore(SCOPED.with(|s| s.borrow_mut().replace(lang)));
    f()
}

/// The translated language a `LANG` value names (BCP 47 `fr-CA`, or a GEDCOM 5.5 name such as
/// `French`), `None` if the library is not translated into it.
pub fn translated(lang: &str) -> Option<&'static str> {
    let lang = lang.trim().to_lowercase();
    let primary = lang.split(['-', '_']).next().unwrap_or("");
    let name = match primary {
        "english" | "anglais" => "en",
        "french" | "français" | "francais" => "fr",
        p => p,
    };
    LANGUAGES.iter().copied().find(|l| *l == name)
}

/// A translation in the current language ([`language`]), or the one given with `locale = …`
/// anywhere among the values: `rust_i18n::t!`, with French elision before the values put in it
/// ([`elide`]).
macro_rules! t {
    ($key:expr) => {
        rust_i18n::t!($key, locale = &*$crate::i18n::language())
    };
    ($key:expr, $($args:tt)+) => {
        t_values!($key; (&*$crate::i18n::language()); []; $($args)+)
    };
}

/// [`t`]'s values, one at a time: `locale` apart, the others marked.
macro_rules! t_values {
    ($key:expr; $lang:expr; [$($n:ident = ($v:expr))*]; locale = $l:expr $(, $($rest:tt)*)?) => {
        t_values!($key; $l; [$($n = ($v))*]; $($($rest)*)?)
    };
    ($key:expr; $lang:expr; [$($n:ident = ($v:expr))*]; $name:ident = $value:expr $(, $($rest:tt)*)?) => {
        t_values!($key; $lang; [$($n = ($v))* $name = ($value)]; $($($rest)*)?)
    };
    ($key:expr; $lang:expr; [$($n:ident = ($v:expr))*];) => {{
        let lang: String = ($lang).to_string();
        $crate::i18n::elide(
            rust_i18n::t!($key, locale = lang.as_str(), $($n = $crate::i18n::Marked(&($v))),*),
            &lang,
        )
    }};
}

/// Marks where a value starts in a translated string, for [`elide`]. A private-use character:
/// it never appears in a catalog or a tree.
const MARK: char = '\u{E000}';

/// A value put in a translation, preceded by [`MARK`].
#[doc(hidden)]
pub struct Marked<'a, T: ?Sized>(pub &'a T);

impl<T: std::fmt::Display + ?Sized> std::fmt::Display for Marked<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{MARK}{}", self.0)
    }
}

/// Removes the marks of the values put in a translation (see [`Marked`]), and in French elides
/// the “de” in front of a value that starts with a vowel: « Décès de %{name} » gives « Décès
/// d’Antonin GUYOT », « Naissance de Louis GUYOT ». Only the catalog’s own “de” changes, never
/// the text of a value. A word in “h” keeps its “de” (« de Henri », « de Hugo »: the elision
/// depends on the word).
pub fn elide<'a>(text: Cow<'a, str>, lang: &str) -> Cow<'a, str> {
    if !text.contains(MARK) {
        return text;
    }
    if lang != "fr" {
        return Cow::Owned(text.replace(MARK, ""));
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_ref();
    while let Some(i) = rest.find(MARK) {
        out.push_str(&rest[..i]);
        rest = &rest[i + MARK.len_utf8()..];
        if starts_with_vowel(rest) {
            for (de, d) in [("de ", "d’"), ("De ", "D’")] {
                let word_start = out.len() >= de.len()
                    && out.ends_with(de)
                    && out[..out.len() - de.len()].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
                if word_start {
                    out.truncate(out.len() - de.len());
                    out.push_str(d);
                    break;
                }
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// Whether a French word starting `text` takes an elision before it: a vowel, or a “y” followed
/// by a consonant (« d’Yvonne », but « de Yolande »).
fn starts_with_vowel(text: &str) -> bool {
    const VOWELS: &str = "aeiouàâäéèêëîïôöùûüæœ";
    let mut chars = text.chars().flat_map(char::to_lowercase);
    match chars.next() {
        Some('y') => chars.next().is_some_and(|c| c.is_alphabetic() && !VOWELS.contains(c) && c != 'y'),
        Some(c) => VOWELS.contains(c),
        None => false,
    }
}

/// Whether `n` takes the singular in the current language: only 1 in English, 0 and 1 in French.
pub fn is_one(n: usize) -> bool {
    match &*language() {
        "fr" => n <= 1,
        _ => n == 1,
    }
}

/// Text of a key built at run time (`tag.BIRT`…), `None` if the catalog has none.
pub(crate) fn lookup(key: &str) -> Option<Cow<'static, str>> {
    lookup_in(key, &language())
}

/// Text of a key built at run time, in the given language.
pub(crate) fn lookup_in(key: &str, lang: &str) -> Option<Cow<'static, str>> {
    crate::_rust_i18n_try_translate(lang, key)
}

/// A string that depends on a number: `tn!(key, n)` reads `key.one` or `key.other`, with the
/// number as `%{n}` and any other named values, like `t!`.
macro_rules! tn {
    ($key:literal, $n:expr $(, $name:ident = $value:expr)* $(,)?) => {{
        let n: usize = $n;
        if $crate::i18n::is_one(n) {
            t!(concat!($key, ".one"), n = n $(, $name = $value)*)
        } else {
            t!(concat!($key, ".other"), n = n $(, $name = $value)*)
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(template: &str, value: &str, lang: &str) -> String {
        let text = template.replace("%{name}", &Marked(value).to_string());
        elide(Cow::Owned(text), lang).into_owned()
    }

    #[test]
    fn french_elides_de_before_a_vowel() {
        assert_eq!(put("Décès de %{name}", "Antonin GUYOT", "fr"), "Décès d’Antonin GUYOT");
        assert_eq!(put("Décès de %{name}", "Élisabeth GUÉRAULT", "fr"), "Décès d’Élisabeth GUÉRAULT");
        assert_eq!(put("De %{name} à Paris", "Alger", "fr"), "D’Alger à Paris");
        assert_eq!(put("Décès de %{name}", "Yvonne OHL", "fr"), "Décès d’Yvonne OHL");
        assert_eq!(put("Décès de %{name}", "Yolande", "fr"), "Décès de Yolande");
        assert_eq!(put("Décès de %{name}", "Louis GUYOT", "fr"), "Décès de Louis GUYOT");
        assert_eq!(put("Décès de %{name}", "Henri GUYOT", "fr"), "Décès de Henri GUYOT");
        assert_eq!(put("Décès de %{name}", "1850", "fr"), "Décès de 1850");
        // Only the catalog's own “de”, as a word.
        assert_eq!(put("Monde %{name}", "Antonin", "fr"), "Monde Antonin");
        assert_eq!(put("%{name}", "de Antonin", "fr"), "de Antonin");
    }

    #[test]
    fn other_languages_only_lose_the_marks() {
        assert_eq!(put("Death of %{name}", "Antonin", "en"), "Death of Antonin");
        assert_eq!(put("Death of %{name}", "Antonin", "fr"), "Death of Antonin");
    }

    #[test]
    fn values_go_through_the_macro() {
        let text = with_language("fr", || t!("leads.reason.born", years = "1850"));
        assert!(!text.contains(MARK));
    }
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    /// Keys the code builds at run time, by prefix.
    const DYNAMIC: &[&str] = &["tag.", "enum.", "struct.", "month.", "month_name.", "date_keyword.", "language."];

    fn catalog() -> BTreeMap<String, BTreeMap<String, String>> {
        let backend = crate::_rust_i18n_backend();
        let mut out: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for lang in LANGUAGES {
            for (key, text) in backend.messages_for_locale(lang).unwrap_or_default() {
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
                let Some(text) = texts.get(lang) else { panic!("`{key}` has no {lang} text") };
                assert!(!text.trim().is_empty(), "`{key}` is empty in {lang}");
                assert_eq!(placeholders(text), placeholders(&texts["en"]), "`{key}`: placeholders differ in {lang}");
            }
        }
    }

    /// Every key the code asks for exists in the catalog, and every key in the catalog is still
    /// used somewhere (keys built at run time are listed in `DYNAMIC`).
    #[test]
    fn code_and_catalog_agree() {
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
                    while let Some(at) = rest.find("!(") {
                        let name = rest[..at].rsplit(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap();
                        rest = &rest[at + 2..];
                        // `word!` (kinship.rs) names three keys, one per sex.
                        if name == "word" {
                            let args = rest.split(')').next().unwrap();
                            for key in args.split('"').skip(1).step_by(2) {
                                assert!(catalog.contains_key(key), "{}: `{key}` is not in locales/", path.display());
                                used.insert(key.to_string());
                            }
                            continue;
                        }
                        // `pick!` (timeline.rs): the first string is a key with one text per sex.
                        if name == "pick" {
                            let key = rest.split('"').nth(1).unwrap();
                            for s in ["m", "f", "u"] {
                                let key = format!("{key}.{s}");
                                assert!(catalog.contains_key(&key), "{}: `{key}` is not in locales/", path.display());
                                used.insert(key);
                            }
                            continue;
                        }
                        let Some(chunk) = rest.trim_start().strip_prefix('"') else { continue };
                        let key = chunk.split('"').next().unwrap();
                        let keys = match name {
                            "t" => vec![key.to_string()],
                            "tn" => vec![format!("{key}.one"), format!("{key}.other")],
                            _ => continue, // another macro, such as `format!`
                        };
                        for key in keys {
                            assert!(catalog.contains_key(&key), "{}: `{key}` is not in locales/", path.display());
                            used.insert(key);
                        }
                    }
                }
            }
        }
        for key in catalog.keys() {
            let dynamic = DYNAMIC.iter().any(|prefix| key.starts_with(prefix));
            assert!(dynamic || used.contains(key), "`{key}` is in locales/ but no longer used");
        }
    }
}
