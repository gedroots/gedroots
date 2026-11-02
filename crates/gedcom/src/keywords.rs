//! Search keywords from earlier versions.
//!
//! The search language (`born:1850`, `without:source`, `sex:f`…) is technical: it is in
//! English whatever the interface language, and only the labels shown for it are translated.
//! Earlier versions used French keywords (`né:1850`, `sans:source`); searches saved in a file
//! with those keywords are converted when they are read, as are the French variables of
//! file-name templates (`{nom}`) and search-site URLs (`{prenom}`).

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

/// Media file-name variables of earlier versions (`{nom:simple}` → `{surname:simple}`).
pub(crate) const MEDIA_VARIABLES: &[(&str, &str)] = &[
    ("annee", "year"),
    ("lieu", "place"),
    ("nom", "surname"),
    ("prenoms", "given_names"),
    ("prenom", "given"),
    ("conjoint", "spouse"),
    ("fratrie", "branch"),
    ("titre", "title"),
];

/// Search-site variables of earlier versions (`{prenom_pere}` → `{father_given}`).
pub(crate) const WEB_VARIABLES: &[(&str, &str)] = &[
    ("nom", "surname"),
    ("prenom", "given"),
    ("naissance", "birth"),
    ("deces", "death"),
    ("lieu", "place"),
    ("nom_pere", "father_surname"),
    ("prenom_pere", "father_given"),
    ("nom_mere", "mother_surname"),
    ("prenom_mere", "mother_given"),
    ("nom_conjoint", "spouse_surname"),
    ("prenom_conjoint", "spouse_given"),
];

/// The template with the French variables of earlier versions in English. Only the text
/// between braces changes, options included (`{nom:simple}`).
pub(crate) fn template_from_french(template: &str, variables: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..=start]);
        rest = &rest[start + 1..];
        let Some(end) = rest.find('}') else { break };
        let inner = &rest[..end];
        let (name, opts) = inner.split_once(':').map_or((inner, None), |(n, o)| (n, Some(o)));
        match variables.iter().find(|(fr, _)| *fr == name.trim()) {
            Some((_, en)) => {
                out.push_str(en);
                if let Some(o) = opts {
                    out.push(':');
                    out.push_str(o);
                }
            }
            None => out.push_str(inner),
        }
        out.push('}');
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
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

    #[test]
    fn french_template_variables_become_english() {
        assert_eq!(
            template_from_french("{sosa:6}_{fratrie}_{type:1}_{nom:simple}_{prenom}_{n}", MEDIA_VARIABLES),
            "{sosa:6}_{branch}_{type:1}_{surname:simple}_{given}_{n}"
        );
        assert_eq!(
            template_from_french("{annee} {prenoms} {surname}", MEDIA_VARIABLES),
            "{year} {given_names} {surname}"
        );
        assert_eq!(
            template_from_french("https://x.org/?nom={nom}&p={prenom_pere}&q={", WEB_VARIABLES),
            "https://x.org/?nom={surname}&p={father_given}&q={"
        );
    }
}
