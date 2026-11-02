//! Recherche en ligne (Individu › Rechercher sur ▸) : des sites de recherche
//! décrits par un modèle d'URL dont les variables (`{surname}`, `{given}`,
//! `{birth}`…) sont remplacées par les données de la personne.
//!
//! Les sites sont propres au fichier, rangés dans l'en-tête par l'extension
//! `_GR_WEB` (voir docs/extensions.md) et réglés dans Réglages du fichier ›
//! Recherche :
//!
//! ```text
//! 1 _GR_WEB Geneanet
//! 2 _GR_URL https://www.geneanet.org/fonds/individus/?go=1&nom={surname}&prenom={given}
//! ```
//!
//! Sans `_GR_WEB`, GEDRoots propose [`DEFAULT_SITES`] ; un `_GR_WEB` vide (sans
//! `_GR_URL`) indique qu'aucun site n'est voulu.

use crate::document::Gedcom;
use crate::model::Individual;
use crate::node::Node;

pub const WEB_TAG: &str = "_GR_WEB";
/// Sous `_GR_WEB` : modèle d'URL.
const URL_TAG: &str = "_GR_URL";
pub const WEB_URI: &str = "https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_web";

/// Variables proposées par le panneau, dans l'ordre.
pub const WEB_TOKENS: &[&str] = &[
    "surname",
    "given",
    "birth",
    "death",
    "place",
    "father_surname",
    "father_given",
    "mother_surname",
    "mother_given",
    "spouse_surname",
    "spouse_given",
];

/// Un site de recherche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSite {
    pub name: String,
    /// Modèle d'URL avec ses variables (`{surname}`…).
    pub url: String,
}

/// Sites proposés tant que le fichier n'en déclare aucun.
pub const DEFAULT_SITES: &[(&str, &str)] = &[
    (
        "Geneanet",
        "https://www.geneanet.org/fonds/individus/?go=1&nom={surname}&prenom={given}&type_periode=between&from={birth}&to={death}",
    ),
    (
        "FamilySearch",
        "https://www.familysearch.org/search/record/results?q.givenName={given}&q.surname={surname}&q.anyDate.from={birth}&q.anyDate.to={death}",
    ),
];

impl Gedcom {
    /// Sites de recherche du fichier, sinon [`DEFAULT_SITES`].
    pub fn search_sites(&self) -> Vec<SearchSite> {
        if !self.header.children.iter().any(|c| c.tag == WEB_TAG) {
            return DEFAULT_SITES.iter().map(|(n, u)| SearchSite { name: n.to_string(), url: u.to_string() }).collect();
        }
        self.header
            .children_with(WEB_TAG)
            .map(|n| SearchSite { name: n.val().to_string(), url: n.child_val(URL_TAG).unwrap_or("").to_string() })
            .filter(|s| !s.url.is_empty())
            .collect()
    }

    /// Enregistre les sites. Les sites sans URL sont ignorés ; une liste égale
    /// aux sites par défaut n'écrit rien. Renvoie `false` si rien n'a changé.
    pub fn set_search_sites(&mut self, sites: &[SearchSite]) -> bool {
        let sites: Vec<SearchSite> = sites
            .iter()
            .map(|s| SearchSite { name: s.name.trim().to_string(), url: s.url.trim().to_string() })
            .filter(|s| !s.url.is_empty())
            .map(|s| SearchSite {
                name: if s.name.is_empty() {
                    crate::host(&s.url).trim_start_matches("www.").to_string()
                } else {
                    s.name
                },
                ..s
            })
            .collect();
        if sites == self.search_sites() {
            return false;
        }
        self.header.children.retain(|c| c.tag != WEB_TAG);
        self.undeclare_extension(WEB_TAG);
        self.undeclare_extension(URL_TAG);
        let defaults: Vec<SearchSite> =
            DEFAULT_SITES.iter().map(|(n, u)| SearchSite { name: n.to_string(), url: u.to_string() }).collect();
        if sites == defaults {
            return true;
        }
        self.declare_extension(WEB_TAG, WEB_URI);
        if sites.is_empty() {
            // Aucun site : un `_GR_WEB` vide, pour ne pas retomber sur les sites par défaut.
            self.header.insert_child(Node::new(WEB_TAG));
            return true;
        }
        self.declare_extension(URL_TAG, WEB_URI);
        for s in &sites {
            self.header.insert_child(Node::with_value(WEB_TAG, &s.name).push(Node::with_value(URL_TAG, &s.url)));
        }
        true
    }

    /// URL de recherche de l'individu `xref` sur un site : chaque variable est
    /// remplacée par sa valeur encodée (vide si inconnue). Les accolades qui ne
    /// sont pas une variable connue restent telles quelles.
    pub fn search_url(&self, template: &str, xref: &str) -> String {
        let values = self.search_values(xref);
        let mut out = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(start) = rest.find('{') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            match after.find('}').map(|end| (&after[..end], end)) {
                Some((name, end)) if WEB_TOKENS.contains(&name) => {
                    let v = values.iter().find(|(k, _)| *k == name).map(|(_, v)| v.as_str()).unwrap_or("");
                    out.push_str(&encode(v));
                    rest = &after[end + 1..];
                }
                _ => {
                    out.push('{');
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// Valeur de chaque variable pour l'individu `xref`.
    fn search_values(&self, xref: &str) -> Vec<(&'static str, String)> {
        let Some(node) = self.get(xref) else { return Vec::new() };
        let p = Individual::new(node);
        let year = |d: Option<crate::Date>| d.and_then(|d| d.year()).map(|y| y.to_string()).unwrap_or_default();
        let place = p.birth().and_then(|e| e.place().map(|pl| self.commune(pl))).unwrap_or_default();
        let (father, mother) = self.parents(xref);
        let spouse = self.spouses_and_children(xref).into_iter().find_map(|(s, _)| s);
        let names = |i: Option<Individual>| i.map(|i| (i.surname(), i.given())).unwrap_or_default();
        let (nf, pf) = names(father);
        let (nm, pm) = names(mother);
        let (nc, pc) = names(spouse);
        vec![
            ("surname", p.surname()),
            ("given", p.given()),
            ("birth", year(p.birth().and_then(|e| e.date()))),
            ("death", year(p.death().and_then(|e| e.date()))),
            ("place", place),
            ("father_surname", nf),
            ("father_given", pf),
            ("mother_surname", nm),
            ("mother_given", pm),
            ("spouse_surname", nc),
            ("spouse_given", pc),
        ]
    }

    /// Commune d'un lieu : la juridiction « Commune » (ou « Ville ») du format
    /// des lieux, sinon la première partie non vide.
    fn commune(&self, place: &str) -> String {
        let parts: Vec<&str> = place.split(',').map(str::trim).collect();
        let form = self.place_form();
        let key = |s: &str| s.to_lowercase();
        if form.len() == parts.len()
            && let Some(i) = form.iter().position(|f| matches!(key(f).as_str(), "commune" | "ville" | "city" | "town"))
            && !parts[i].is_empty()
        {
            return parts[i].to_string();
        }
        parts.iter().find(|p| !p.is_empty()).map(|p| p.to_string()).unwrap_or_default()
    }
}

/// Encodage d'une valeur dans une URL (UTF-8, `%20` pour les espaces).
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "0 HEAD
1 GEDC
2 VERS 7.0
1 PLAC
2 FORM Lieudit, Commune, Code_Postal, Département, Région, Pays
0 @I1@ INDI
1 NAME Jean Marie /D'ÉTÉ/
1 BIRT
2 DATE ABT 1850
2 PLAC Epeluy, Sury-le-Comtal, 42450, Loire, Rhône-Alpes, France
1 DEAT
2 DATE 3 MAR 1912
1 FAMC @F1@
1 FAMS @F2@
0 @I2@ INDI
1 NAME Pierre /D'ÉTÉ/
1 FAMS @F1@
0 @I3@ INDI
1 NAME Anne /MARTIN/
1 FAMS @F2@
0 @F1@ FAM
1 HUSB @I2@
1 CHIL @I1@
0 @F2@ FAM
1 HUSB @I1@
1 WIFE @I3@
0 TRLR
";

    #[test]
    fn fills_template() {
        let g = Gedcom::parse(SAMPLE).unwrap();
        let url = g.search_url(
            "https://x.org/?n={surname}&p={given}&b={birth}&d={death}&l={place}&pp={father_given}&m={mother_surname}&c={spouse_given}&k={inconnu}",
            "I1",
        );
        assert_eq!(
            url,
            "https://x.org/?n=D%27%C3%89T%C3%89&p=Jean%20Marie&b=1850&d=1912&l=Sury-le-Comtal&pp=Pierre&m=&c=Anne&k={inconnu}"
        );
        // Accolade isolée et individu absent.
        assert_eq!(g.search_url("a{b", "I1"), "a{b");
        assert_eq!(g.search_url("https://x.org/?n={surname}", "I9"), "https://x.org/?n=");
    }

    #[test]
    fn defaults_then_custom_sites() {
        let mut g = Gedcom::parse(SAMPLE).unwrap();
        let defaults = g.search_sites();
        assert_eq!(defaults.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Geneanet", "FamilySearch"]);
        // Rien ne change : rien n'est écrit.
        assert!(!g.set_search_sites(&defaults));
        assert!(g.header.child(WEB_TAG).is_none());

        let mine = vec![
            SearchSite { name: "Geneanet".into(), url: defaults[0].url.clone() },
            SearchSite { name: " ".into(), url: "https://memoiredeshommes.fr/?nom={surname}".into() },
            SearchSite { name: "Vide".into(), url: "  ".into() },
        ];
        assert!(g.set_search_sites(&mine));
        let sites = g.search_sites();
        assert_eq!(sites.len(), 2);
        assert_eq!(sites[1].name, "memoiredeshommes.fr");
        let text = g.to_gedcom_string();
        assert!(text.contains("2 TAG _GR_WEB "));
        assert!(text.contains("1 _GR_WEB memoiredeshommes.fr\n2 _GR_URL https://memoiredeshommes.fr/?nom={surname}"));

        // Plus aucun site : le menu disparaît.
        assert!(g.set_search_sites(&[]));
        assert!(g.search_sites().is_empty());
        assert!(g.to_gedcom_string().contains("\n1 _GR_WEB\n"));
        assert!(!g.set_search_sites(&[]));

        // Retour aux sites par défaut : les balises et leur déclaration disparaissent.
        assert!(g.set_search_sites(&defaults));
        let text = g.to_gedcom_string();
        assert!(!text.contains("_GR_WEB") && !text.contains("_GR_URL"));
    }
}
