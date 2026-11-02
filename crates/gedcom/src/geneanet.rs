//! Liens vers Geneanet.
//!
//! Geneanet n'offre pas d'API publique de recherche : on ouvre sa page de
//! recherche dans le navigateur, préremplie avec le nom, le prénom et la
//! période de vie de la personne.

use crate::{Individual, Node};

/// Page d'import d'un fichier GEDCOM sur Geneanet.
pub const IMPORT_URL: &str = "https://www.geneanet.org/creer-votre-arbre/gedcom";

/// Recherche Geneanet d'un individu (`INDI`) : nom, prénom, années de naissance
/// et de décès quand elles sont connues.
pub fn search_url(indi: &Node) -> String {
    let p = Individual::new(indi);
    let death = p.death().and_then(|e| e.date()).and_then(|d| d.year());
    let year = |y: Option<i32>| y.map(|y| y.to_string()).unwrap_or_default();
    let params = [
        ("go", "1".to_string()),
        ("nom", p.surname()),
        ("prenom", p.given()),
        ("type_periode", "between".to_string()),
        ("from", year(p.birth_year())),
        ("to", year(death)),
    ];
    let query: Vec<String> =
        params.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| format!("{k}={}", encode(v))).collect();
    format!("https://www.geneanet.org/fonds/individus/?{}", query.join("&"))
}

/// Encodage d'un paramètre d'URL (UTF-8, espaces en `+`).
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Gedcom;

    #[test]
    fn search_url_with_names_and_years() {
        let g = Gedcom::parse(
            "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 NAME Jean Marie /D'ÉTÉ/\n1 BIRT\n2 DATE ABT 1850\n1 DEAT\n2 DATE 3 MAR 1912\n0 @I2@ INDI\n1 NAME /X/\n0 TRLR\n",
        )
        .unwrap();
        assert_eq!(
            search_url(g.get("I1").unwrap()),
            "https://www.geneanet.org/fonds/individus/?go=1&nom=D%27%C3%89T%C3%89&prenom=Jean+Marie&type_periode=between&from=1850&to=1912"
        );
        assert_eq!(
            search_url(g.get("I2").unwrap()),
            "https://www.geneanet.org/fonds/individus/?go=1&nom=X&type_periode=between"
        );
    }
}
