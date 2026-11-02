//! Recherche de lieux réels dans OpenStreetMap, à la demande : saisie d'un nouveau lieu
//! et « Localiser » sur la fiche d'un lieu. Le service est Photon (komoot), fait pour la
//! recherche pendant la saisie, limité aux lieux habités (`osm_tag=place`). Une requête
//! à la fois, après une pause de saisie ; jamais en masse.

use gedcom::gazetteer::Candidate;
use std::sync::OnceLock;
use std::time::Duration;

const DEFAULT_GEOCODER: &str = "https://photon.komoot.io/api/";
const USER_AGENT: &str =
    concat!("GEDRoots/", env!("CARGO_PKG_VERSION"), " (+https://github.com/antoninguyot/gedroots)");

/// Pause de saisie avant de chercher.
pub const TYPING_PAUSE: Duration = Duration::from_millis(450);
/// Délai entre deux requêtes d'une même recherche.
pub const PAUSE: Duration = Duration::from_millis(1100);

/// Recherche désactivée (`GEDROOTS_OFFLINE`).
pub fn offline() -> bool {
    std::env::var_os("GEDROOTS_OFFLINE").is_some()
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(15))
            .try_proxy_from_env(true)
            .build()
    })
}

/// Lieux habités (communes, hameaux, villes…) qui correspondent à la recherche, du plus
/// probable au moins probable. Bloquant : à appeler hors du fil de l'interface.
pub fn search(q: &str, limit: usize) -> Result<Vec<Candidate>, String> {
    let url = std::env::var("GEDROOTS_GEOCODER").unwrap_or_else(|_| DEFAULT_GEOCODER.into());
    let resp = agent()
        .get(&url)
        .query("q", q)
        .query("limit", &limit.to_string())
        .query("lang", &crate::i18n::language())
        .query("osm_tag", "place")
        .call()
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    Ok(parse(&v))
}

/// Réponse GeoJSON de Photon.
fn parse(v: &serde_json::Value) -> Vec<Candidate> {
    let Some(features) = v.get("features").and_then(|f| f.as_array()) else { return Vec::new() };
    features
        .iter()
        .filter_map(|f| {
            let coords = f.get("geometry")?.get("coordinates")?.as_array()?;
            let (lon, lat) = (coords.first()?.as_f64()?, coords.get(1)?.as_f64()?);
            let p = f.get("properties")?;
            let s = |k: &str| p.get(k).and_then(|x| x.as_str()).filter(|x| !x.trim().is_empty());
            let name = s("name")?.trim().to_string();
            // Le lieu lui-même, rangé à son niveau : commune, ou lieu-dit dans sa commune.
            let kind = match s("osm_value").unwrap_or("") {
                k @ ("city" | "town" | "village" | "municipality") => k,
                k @ ("hamlet" | "isolated_dwelling" | "locality" | "neighbourhood" | "quarter") => k,
                _ => "locality",
            };
            let mut address = vec![(kind.to_string(), name.clone())];
            if !matches!(kind, "city" | "town" | "village" | "municipality")
                && let Some(c) = s("city").filter(|c| *c != name)
            {
                address.push(("city".into(), c.to_string()));
            }
            for k in ["postcode", "county", "state", "country"] {
                if let Some(v) = s(k) {
                    address.push((k.to_string(), v.to_string()));
                }
            }
            let exid = match (s("osm_type"), p.get("osm_id").and_then(|x| x.as_u64())) {
                (Some(t), Some(id)) => {
                    let t = match t {
                        "N" => "node",
                        "W" => "way",
                        "R" => "relation",
                        _ => return Some(Candidate { name, lat, lon, address, exid: None }),
                    };
                    Some((id.to_string(), format!("https://www.openstreetmap.org/{t}/")))
                }
                _ => None,
            };
            Some(Candidate { name, lat, lon, address, exid })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_photon() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"features":[
              {"geometry":{"coordinates":[4.1175,48.2978]},"properties":{"osm_type":"R","osm_id":415228,
                "osm_key":"place","osm_value":"village","name":"Saint-Parres-aux-Tertres","county":"Aube",
                "state":"Grand Est","country":"France","postcode":"10410"}},
              {"geometry":{"coordinates":[4.11,48.29]},"properties":{"osm_type":"N","osm_id":8615205840,
                "osm_value":"locality","name":"Cortins de Saint-Parres","city":"Saint-Parres-aux-Tertres",
                "county":"Aube","state":"Grand Est","country":"France","postcode":"10410"}}]}"#,
        )
        .unwrap();
        let found = parse(&v);
        let c = &found[0];
        assert_eq!(c.title(), "Saint-Parres-aux-Tertres");
        assert_eq!(c.context(), "Aube · Grand Est · France");
        assert_eq!(c.exid, Some(("415228".into(), "https://www.openstreetmap.org/relation/".into())));
        let form: Vec<String> =
            ["Lieudit", "Commune", "Code_Postal", "Département", "Région", "Pays"].map(String::from).to_vec();
        assert_eq!(
            found[1].parts(&form),
            vec!["Cortins de Saint-Parres", "Saint-Parres-aux-Tertres", "10410", "Aube", "Grand Est", "France"]
        );
        assert_eq!(found[1].context(), "Saint-Parres-aux-Tertres · Aube · Grand Est · France");
    }
}
