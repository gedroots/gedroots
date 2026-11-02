//! Search keywords from earlier versions.
//!
//! The search language (`born:1850`, `without:source`, `sex:f`…) is technical: it is in
//! English whatever the interface language, and only the labels shown for it are translated.
//! Earlier versions used French keywords (`né:1850`, `sans:source`); searches saved in a file
//! with those keywords are converted when they are read.

use crate::model::sort_key;
use crate::query::{Scope, words};

/// English keyword for a French one, in this zone.
fn key(scope: Scope, k: &str) -> Option<&'static str> {
    Some(match (scope, k) {
        (Scope::Individuals, "ne" | "nee" | "naissance" | "n") => "born",
        (Scope::Individuals, "mort" | "morte" | "deces") => "died",
        (Scope::Individuals, "vecu" | "vivant" | "en") => "alive",
        (Scope::Individuals, "lieu" | "a") => "place",
        (Scope::Individuals, "nom") => "name",
        (Scope::Individuals, "prof" | "profession" | "metier") => "occupation",
        (Scope::Individuals, "sexe") => "sex",
        (Scope::Individuals, "conjoint" | "conjoints" | "epoux") => "spouses",
        (Scope::Individuals, "enfant" | "enfants") => "children",
        (Scope::Individuals, "fratrie" | "frere" | "freres" | "soeur" | "soeurs") => "siblings",
        (Scope::Individuals, "ascendant" | "ascendants" | "ancetres") => "ancestors",
        (Scope::Sources, "acte") => "type",
        (Scope::Sources, "depot" | "archive" | "archives") => "repository",
        (Scope::Places, "dans" | "en") => "in",
        (Scope::Media, "evenement" | "acte" | "fait") => "event",
        (Scope::Media, "pour" | "personne" | "nom") => "person",
        (Scope::Media, "support" | "medi") => "medium",
        (_, "sans") => "without",
        (_, "avec") => "with",
        _ => return None,
    })
}

/// English value for a French one (`sans:famille` → `without:spouse`).
fn value(k: &str, v: &str) -> Option<&'static str> {
    let v = sort_key(v);
    Some(match (k, v.trim_end_matches('s')) {
        ("sex", "h" | "homme") => "m",
        ("sex", "femme") => "f",
        ("sex", "inconnu") => "u",
        ("without" | "with", "famille" | "conjoint") => "spouse",
        ("without" | "with", "mariage") => "marriage",
        ("without" | "with", "coordonnee" | "coord") => "coordinates",
        ("without" | "with", "lien" | "rattachement") => "link",
        ("without" | "with", "cite") => "citation",
        ("format", "lien") => "web",
        _ => return None,
    })
}

/// The query with the French keywords of earlier versions in English. Free words, values
/// typed by the user and English keywords are kept as they are.
pub(crate) fn from_french(scope: Scope, query: &str) -> String {
    let mut out = String::new();
    let mut last = 0;
    for (range, w) in words(query) {
        out.push_str(&query[last..range.start]);
        last = range.end;
        let Some((k, v)) = w.split_once(':').filter(|(k, _)| !k.is_empty() && !k.starts_with('"')) else {
            out.push_str(w);
            continue;
        };
        let k = key(scope, &sort_key(k)).map_or_else(|| k.to_string(), str::to_string);
        let v = value(&k, v).map_or_else(|| v.to_string(), str::to_string);
        out.push_str(&format!("{k}:{v}"));
    }
    out.push_str(&query[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_keywords_become_english() {
        assert_eq!(from_french(Scope::Individuals, "né:1850±5 guyot sexe:h"), "born:1850±5 guyot sex:m");
        assert_eq!(from_french(Scope::Individuals, "sans:famille avec:média"), "without:spouse with:média");
        assert_eq!(from_french(Scope::Individuals, r#"lieu:"Bar sur Aube""#), r#"place:"Bar sur Aube""#);
        assert_eq!(from_french(Scope::Places, "en:Manche sans:coordonnées"), "in:Manche without:coordinates");
        assert_eq!(from_french(Scope::Media, "nom:guyot sans:lien"), "person:guyot without:link");
        assert_eq!(from_french(Scope::Individuals, "born:1850 10:30"), "born:1850 10:30");
    }
}
