//! Export en GEDCOM 5.5.1, pour les logiciels et sites qui ne lisent pas encore
//! GEDCOM 7 (Geneanet, Heredis…).
//!
//! [`export`] part d'une copie du fichier : le fichier ouvert n'est pas modifié.
//! Ce qui change par rapport à GEDCOM 7 :
//! - en-tête : `GEDC.VERS 5.5.1`, `GEDC.FORM LINEAGE-LINKED`, `CHAR UTF-8`,
//!   langue en toutes lettres (`fr` → `French`), plus de `SCHMA` ;
//! - notes partagées : `SNOTE` devient `NOTE` (enregistrement et lien) ;
//! - dates : calendriers échappés (`FRENCH_R` → `@#DFRENCH R@`), `BCE` → `B.C.`,
//!   une date accompagnée d'un `PHRASE` devient une date interprétée
//!   (`INT 1 JAN 1800 (texte)`) ou une phrase seule (`(texte)`) ;
//! - âges : `> 39y` → `>39y`, semaines comptées en jours ;
//! - médias : type MIME → format (`image/jpeg` → `jpeg`), `MEDI` → `TYPE` ;
//! - associations : `ROLE` → `RELA` en clair (`WITN` → `Witness`) ;
//! - valeurs énumérées en minuscules (`PEDI`, `NAME.TYPE`, `RESN`…), `SEX X` → `U` ;
//! - structures propres à GEDCOM 7 sans équivalent retirées (`EXID`, `CREA`,
//!   `CROP`, `NO`, `TRAN`, `SDATE`, `PHRASE`…), ainsi que les pointeurs `@VOID@`
//!   et les extensions internes de GEDRoots (`_GR_*`) ; `UID` devient `_UID` ;
//! - texte : tous les `@` doublés, lignes longues coupées en `CONC`.
//!
//! La variante [`Flavor::Geneanet`] suit en plus ce qu'attend l'import de
//! Geneanet (GeneWeb) : lieux-dits au format `[Lieu-dit] - Commune, …` quand la
//! première juridiction du format des lieux est un lieu-dit, et rôles des
//! témoins dans le vocabulaire de GeneWeb (`GODP`, `Civil officer`…).

use crate::{Gedcom, Node};
use std::sync::Arc;

/// Variante de l'export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    /// GEDCOM 5.5.1 standard.
    Standard,
    /// GEDCOM 5.5.1 adapté à l'import de Geneanet.
    Geneanet,
}

/// Longueur maximale d'une valeur avant découpe en `CONC` (la norme limite la
/// ligne entière à 255 caractères).
const MAX_VALUE: usize = 200;

/// Texte GEDCOM 5.5.1 de `g` (voir le module).
pub fn export(g: &Gedcom, flavor: Flavor) -> String {
    let mut out = g.clone();
    out.stamp_header();
    let form = out.place_form();
    let sub = flavor == Flavor::Geneanet && form.first().is_some_and(|j| is_sublocality(j));
    let cx = Cx { flavor, form: &form, sub };
    out.header = header(&out.header, &cx);
    out.records = out.records.iter().filter_map(|r| convert(r, None, &cx)).map(Arc::new).collect();
    out.format.bom = false;
    out.to_gedcom_string()
}

struct Cx<'a> {
    flavor: Flavor,
    /// Format des lieux du fichier (`HEAD.PLAC.FORM`).
    form: &'a [String],
    /// La première juridiction du format est un lieu-dit à réécrire (Geneanet).
    sub: bool,
}

fn header(h: &Node, cx: &Cx) -> Node {
    let mut h = convert(h, None, cx).unwrap_or_else(|| Node::new("HEAD"));
    h.children.retain(|c| !matches!(c.tag.as_str(), "GEDC" | "CHAR"));
    let at = h.children.iter().position(|c| c.tag == "DATE").map(|i| i + 1).unwrap_or(h.children.len());
    let gedc =
        Node::new("GEDC").push(Node::with_value("VERS", "5.5.1")).push(Node::with_value("FORM", "LINEAGE-LINKED"));
    h.children.insert(at, gedc);
    h.children.insert(at + 1, Node::with_value("CHAR", "UTF-8"));
    if cx.sub {
        // Le lieu-dit passe dans le nom de la commune : une juridiction de moins.
        if let Some(f) = h.child_mut("PLAC").and_then(|p| p.child_mut("FORM")) {
            f.value = Some(cx.form[1..].join(", "));
        }
    }
    h
}

/// Nom de langue GEDCOM 5.5.1 d'une étiquette BCP 47 (`fr`, `fr-FR`).
fn language(tag: &str) -> Option<&'static str> {
    let primary = tag.split('-').next().unwrap_or("").to_ascii_lowercase();
    Some(match primary.as_str() {
        "fr" => "French",
        "en" => "English",
        "de" => "German",
        "es" => "Spanish",
        "it" => "Italian",
        "pt" => "Portuguese",
        "nl" => "Dutch",
        "ca" => "Catalan",
        "pl" => "Polish",
        "ar" => "Arabic",
        "he" => "Hebrew",
        "la" => "Latin",
        "oc" => "Occitan",
        "br" => "Breton",
        "eu" => "Basque",
        "da" => "Danish",
        "sv" => "Swedish",
        "no" | "nb" | "nn" => "Norwegian",
        "fi" => "Finnish",
        "cs" => "Czech",
        "hu" => "Hungarian",
        "ru" => "Russian",
        "el" => "Greek",
        _ => return None,
    })
}

/// Structures de GEDCOM 7 sans équivalent en 5.5.1.
const DROPPED: [&str; 10] = ["EXID", "CREA", "CROP", "NO", "INIL", "MIME", "TRAN", "SDATE", "SCHMA", "PHRASE"];

/// Convertit `n` (et ses sous-structures) ; `None` : à retirer.
fn convert(n: &Node, parent: Option<&str>, cx: &Cx) -> Option<Node> {
    let tag = n.tag.as_str();
    if tag.starts_with("_GR_") || DROPPED.contains(&tag) || n.val() == "@VOID@" {
        return None;
    }
    // Langue : seulement dans l'en-tête et les auteurs.
    if tag == "LANG" && !matches!(parent, Some("HEAD" | "SUBM")) {
        return None;
    }
    if tag == "STAT" && parent == Some("FAMC") {
        return None;
    }
    let mut out = Node { xref: n.xref.clone(), tag: n.tag.clone(), value: n.value.clone(), children: Vec::new() };
    match tag {
        "SNOTE" => out.tag = "NOTE".into(),
        "UID" => out.tag = "_UID".into(),
        "LANG" => out.value = Some(language(n.val())?.into()),
        "SEX" if n.val() == "X" => out.value = Some("U".into()),
        "RESN" => {
            let first = n.val().split(',').next().unwrap_or("").trim().to_ascii_lowercase();
            if first.is_empty() {
                return None;
            }
            out.value = Some(first);
        }
        "PEDI" | "MEDI" => {
            let v = n.val().to_ascii_lowercase();
            if v == "other" {
                return None;
            }
            out.value = Some(v);
            if tag == "MEDI" {
                out.tag = "TYPE".into();
            }
        }
        "TYPE" if parent == Some("NAME") => {
            let v = n.val().to_ascii_lowercase();
            if v == "other" {
                return None;
            }
            out.value = Some(v);
        }
        "FORM" if parent == Some("FILE") => out.value = Some(file_format(n.val())),
        "DATE" => out.value = date(n.val(), n.child_val("PHRASE")),
        "AGE" => out.value = Some(age(n.val())),
        "PLAC" if cx.sub && n.value.is_some() && parent != Some("HEAD") => {
            out.value = Some(geneanet_place(n.val(), n.child_val("FORM"), cx.form))
        }
        _ => {}
    }
    // Lien vers un média : pas de sous-structures en 5.5.1 (hors extensions).
    let media_link = tag == "OBJE" && n.pointer().is_some();
    for c in &n.children {
        if media_link && !c.tag.starts_with('_') {
            continue;
        }
        if tag == "ASSO" && c.tag == "ROLE" {
            out.children.push(Node::with_value("RELA", relation(c, parent, cx.flavor)));
            continue;
        }
        if c.tag == "CONT" || c.tag == "CONC" {
            out.children.push(c.clone());
            continue;
        }
        if let Some(c) = convert(c, Some(tag), cx) {
            out.children.push(c);
        }
    }
    if tag == "DATE" && out.value.is_none() && out.children.is_empty() {
        return None;
    }
    if tag != "DATE" {
        escape_text(&mut out);
    }
    Some(out)
}

/// Format 5.5.1 d'un type MIME (`image/jpeg` → `jpeg`, `text/html` → `html`).
fn file_format(mime: &str) -> String {
    let sub = mime.rsplit('/').next().unwrap_or(mime).to_ascii_lowercase();
    match sub.as_str() {
        "jpeg" | "jpg" | "pjpeg" => "jpg".into(),
        "tiff" => "tif".into(),
        "x-wav" | "wave" => "wav".into(),
        "mpeg" => "mp3".into(),
        "plain" => "txt".into(),
        _ => sub,
    }
}

/// Date GEDCOM 7 → 5.5.1 ; `phrase` : texte libre qui l'accompagne.
fn date(v: &str, phrase: Option<&str>) -> Option<String> {
    let words: Vec<String> = v
        .split_whitespace()
        .map(|w| match w {
            "GREGORIAN" => "@#DGREGORIAN@".into(),
            "JULIAN" => "@#DJULIAN@".into(),
            "FRENCH_R" => "@#DFRENCH R@".into(),
            "HEBREW" => "@#DHEBREW@".into(),
            "BCE" => "B.C.".into(),
            w => w.to_string(),
        })
        .collect();
    let d = words.join(" ");
    let phrase = phrase.map(|p| p.replace('(', "[").replace(')', "]")).filter(|p| !p.trim().is_empty());
    const MODIFIERS: [&str; 8] = ["ABT", "CAL", "EST", "BEF", "AFT", "BET", "FROM", "TO"];
    match phrase {
        None if d.is_empty() => None,
        None => Some(d),
        Some(p) if d.is_empty() => Some(format!("({p})")),
        Some(p) if !MODIFIERS.contains(&words[0].as_str()) => Some(format!("INT {d} ({p})")),
        Some(_) => Some(d),
    }
}

/// Âge GEDCOM 7 → 5.5.1 : `> 39y 2w` → `>39y 14d`.
fn age(v: &str) -> String {
    let v = v.trim();
    let (sign, rest) = match v.chars().next() {
        Some(c @ ('<' | '>')) => (c.to_string(), v[1..].trim_start()),
        _ => (String::new(), v),
    };
    let (mut parts, mut days) = (Vec::new(), 0u32);
    for w in rest.split_whitespace() {
        if let Some(n) = w.strip_suffix('w').and_then(|n| n.parse::<u32>().ok()) {
            days += n * 7;
        } else if let Some(n) = w.strip_suffix('d').and_then(|n| n.parse::<u32>().ok()) {
            days += n;
        } else {
            parts.push(w.to_string());
        }
    }
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    format!("{sign}{}", parts.join(" "))
}

/// Rôle d'un témoin (`ASSO.ROLE`) en clair pour `RELA`. `event` : la structure
/// qui porte l'association (un baptême a un officiant religieux).
fn relation(role: &Node, event: Option<&str>, flavor: Flavor) -> String {
    let r = role.val().trim();
    let phrase = role.child_val("PHRASE").map(str::trim).filter(|p| !p.is_empty());
    if flavor == Flavor::Geneanet {
        // Vocabulaire lu par GeneWeb (`ged2gwb`) ; le reste devient un témoin.
        let religious =
            matches!(event, Some("BAPM" | "CHR" | "CONF" | "FCOM" | "BURI" | "ORDN" | "BARM" | "BASM" | "BLES"));
        return match r {
            "GODP" => "GODP",
            "CLERGY" => "Religious officer",
            "OFFICIATOR" if religious => "Religious officer",
            "OFFICIATOR" => "Civil officer",
            "OTHER" => "Other",
            _ => "Witness",
        }
        .into();
    }
    match r {
        "WITN" => "Witness",
        "GODP" => "Godparent",
        "OFFICIATOR" => "Officiator",
        "CLERGY" => "Clergy",
        "FATH" => "Father",
        "MOTH" => "Mother",
        "PARENT" => "Parent",
        "HUSB" => "Husband",
        "WIFE" => "Wife",
        "SPOU" => "Spouse",
        "CHIL" => "Child",
        "FRIEND" => "Friend",
        "NGHBR" => "Neighbor",
        "MULTIPLE" => "Multiple",
        "OTHER" => return phrase.unwrap_or("Other").to_string(),
        other => return phrase.unwrap_or(other).to_string(),
    }
    .into()
}

/// Nom de juridiction qui désigne une subdivision de commune (lieu-dit, hameau…).
fn is_sublocality(name: &str) -> bool {
    let n: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'é' | 'è' | 'ê' => 'e',
            'à' | 'â' => 'a',
            'ô' => 'o',
            c => c,
        })
        .collect();
    matches!(n.as_str(), "lieudit" | "lieuxdits" | "hameau" | "quartier" | "subdivision" | "ecart" | "souscommune")
}

/// Lieu au format de Geneanet : `[Lieu-dit] - Commune, …`, ou `Commune, …` sans
/// lieu-dit. `own` : format propre du lieu (`PLAC.FORM`), sinon celui du fichier.
fn geneanet_place(v: &str, own: Option<&str>, form: &[String]) -> String {
    if let Some(f) = own
        && !f.split(',').next().is_some_and(is_sublocality)
    {
        return v.to_string();
    }
    let len = own.map(|f| f.split(',').count()).unwrap_or(form.len());
    let parts: Vec<&str> = v.split(',').map(str::trim).collect();
    if parts.len() != len || parts.len() < 2 {
        return v.to_string();
    }
    let rest = parts[1..].join(", ");
    match parts[0] {
        "" => rest,
        sub => format!("[{sub}] - {rest}"),
    }
}

/// Échappe le texte d'une structure pour 5.5.1 (tous les `@` doublés) et coupe
/// les lignes trop longues en `CONC`. Les pointeurs ne changent pas.
fn escape_text(n: &mut Node) {
    if n.pointer().is_some() {
        return;
    }
    let esc = |v: &str| {
        // GEDCOM 7 n'échappe que le `@` initial (`@@`).
        let literal = v.strip_prefix("@@").map(|s| format!("@{s}")).unwrap_or_else(|| v.to_string());
        literal.replace('@', "@@")
    };
    let first = n.value.as_deref().map(esc);
    let conts: Vec<Option<String>> =
        n.children.iter().filter(|c| c.tag == "CONT").map(|c| c.value.as_deref().map(esc)).collect();
    let long = |v: &Option<String>| v.as_ref().is_some_and(|v| v.len() > MAX_VALUE);
    let has_conc = n.children.iter().any(|c| c.tag == "CONC");
    if has_conc || (!long(&first) && !conts.iter().any(long)) {
        // Rien à couper (ou déjà coupé, fichier 5.5.1 importé) : on échappe sur place.
        n.value = first;
        let mut it = conts.into_iter();
        for c in n.children.iter_mut().filter(|c| c.tag == "CONT") {
            c.value = it.next().flatten();
        }
        return;
    }
    let mut lines = Vec::new();
    let mut chunks = split(first.as_deref().unwrap_or(""));
    n.value = Some(chunks.remove(0)).filter(|s| !s.is_empty());
    lines.extend(chunks.into_iter().map(|c| Node::with_value("CONC", c)));
    for c in conts {
        let mut chunks = split(c.as_deref().unwrap_or(""));
        let head = chunks.remove(0);
        lines.push(Node { tag: "CONT".into(), value: Some(head).filter(|s| !s.is_empty()), ..Default::default() });
        lines.extend(chunks.into_iter().map(|c| Node::with_value("CONC", c)));
    }
    n.children.retain(|c| c.tag != "CONT");
    n.children.splice(0..0, lines);
}

/// Coupe `s` en morceaux d'au plus [`MAX_VALUE`] octets environ, jamais à côté
/// d'une espace (que certains lecteurs suppriment) ni au milieu d'un `@@`.
fn split(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while rest.len() > MAX_VALUE {
        let ok = |i: usize| {
            rest.is_char_boundary(i) && !rest[..i].ends_with([' ', '@']) && !rest[i..].starts_with([' ', '@'])
        };
        let cut = (1..=MAX_VALUE).rev().find(|&i| ok(i)).or_else(|| (MAX_VALUE..rest.len()).find(|&i| ok(i)));
        let Some(cut) = cut else { break };
        out.push(rest[..cut].to_string());
        rest = &rest[cut..];
    }
    out.push(rest.to_string());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "0 HEAD
1 GEDC
2 VERS 7.0
1 SCHMA
2 TAG _GR_ROOT https://gedroots.org/ext/root
1 LANG fr
1 PLAC
2 FORM Lieudit, Commune, Code_Postal, Département, Région, Pays
1 _GR_ROOT @I1@
0 @I1@ INDI
1 NAME Jean /DUPONT/
2 TYPE BIRTH
1 SEX X
1 RESN CONFIDENTIAL, PRIVACY
1 UID 0b6a
1 EXID 123
2 TYPE https://example.org
1 BIRT
2 DATE FRENCH_R 30 GERM 4
2 PLAC Epeluy, Sury-le-Comtal, 42450, Loire, Rhône-Alpes, France
2 ASSO @I2@
3 ROLE WITN
2 ASSO @VOID@
3 PHRASE inconnu
1 BAPM
2 DATE 2 MAY 1796
3 PHRASE le lendemain
2 PLAC , Sury-le-Comtal, 42450, Loire, Rhône-Alpes, France
2 ASSO @I2@
3 ROLE OFFICIATOR
1 DEAT
2 DATE
3 PHRASE pendant la guerre
2 AGE > 39y 2w 1d
1 FAMC @F1@
2 PEDI BIRTH
2 STAT CHALLENGED
1 OBJE @M1@
2 CROP
3 TOP 10
1 SNOTE @N1@
1 NOTE courriel jean@exemple.fr
2 CONT @@ début
1 CREA
2 DATE 1 JAN 2026
0 @I2@ INDI
1 NAME Pierre /MARTIN/
0 @F1@ FAM
1 CHIL @I1@
0 @M1@ OBJE
1 FILE Actes/naissance.jpg
2 FORM image/jpeg
3 MEDI PHOTO
2 TITL Acte
0 @N1@ SNOTE Une note partagée
0 TRLR
";

    fn lines(s: &str) -> Vec<&str> {
        s.lines().collect()
    }

    #[test]
    fn standard() {
        let g = Gedcom::parse(SAMPLE).unwrap();
        let out = export(&g, Flavor::Standard);
        let l = lines(&out);
        let has = |x: &str| l.contains(&x);
        assert!(has("1 GEDC") && has("2 VERS 5.5.1") && has("2 FORM LINEAGE-LINKED") && has("1 CHAR UTF-8"));
        assert!(has("1 LANG French"));
        assert!(!out.contains("SCHMA") && !out.contains("_GR_") && !out.contains("EXID") && !out.contains("CREA"));
        assert!(!out.contains("VOID") && !out.contains("PHRASE") && !out.contains("CROP") && !out.contains("STAT"));
        assert!(has("2 TYPE birth") && has("1 SEX U") && has("1 RESN confidential") && has("1 _UID 0b6a"));
        assert!(has("2 DATE @#DFRENCH R@ 30 GERM 4"));
        assert!(has("2 DATE INT 2 MAY 1796 (le lendemain)"));
        assert!(has("2 DATE (pendant la guerre)"));
        assert!(has("2 AGE >39y 15d"));
        assert!(has("2 PEDI birth"));
        assert!(has("3 RELA Witness") && has("3 RELA Officiator") && !out.contains("ROLE"));
        assert!(has("1 OBJE @M1@") && has("2 FORM jpg") && has("3 TYPE photo"));
        assert!(has("1 NOTE @N1@") && has("0 @N1@ NOTE Une note partagée"));
        assert!(has("1 NOTE courriel jean@@exemple.fr") && has("2 CONT @@ début"));
        // Lieux inchangés hors Geneanet.
        assert!(has("2 PLAC Epeluy, Sury-le-Comtal, 42450, Loire, Rhône-Alpes, France"));
        assert!(!out.starts_with('\u{feff}'));
        // Le fichier d'origine n'a pas bougé.
        assert_eq!(g.version(), "7.0");
    }

    #[test]
    fn geneanet() {
        let g = Gedcom::parse(SAMPLE).unwrap();
        let out = export(&g, Flavor::Geneanet);
        let l = lines(&out);
        let has = |x: &str| l.contains(&x);
        assert!(has("2 PLAC [Epeluy] - Sury-le-Comtal, 42450, Loire, Rhône-Alpes, France"));
        assert!(has("2 PLAC Sury-le-Comtal, 42450, Loire, Rhône-Alpes, France"));
        assert!(has("2 FORM Commune, Code_Postal, Département, Région, Pays"));
        assert!(has("3 RELA Witness") && has("3 RELA Religious officer"));
    }

    #[test]
    fn places_without_sublocality_stay() {
        let form = ["Commune".to_string(), "Pays".to_string()];
        assert!(!is_sublocality(&form[0]));
        assert!(is_sublocality("Lieu-dit") && is_sublocality("Hameau") && is_sublocality("lieudit"));
        let form = ["Lieu-dit".to_string(), "Commune".to_string(), "Pays".to_string()];
        // Nombre de juridictions inattendu : laissé tel quel.
        assert_eq!(geneanet_place("Paris, France", None, &form), "Paris, France");
        assert_eq!(geneanet_place("La Côte, Unias, France", None, &form), "[La Côte] - Unias, France");
        // Format propre au lieu, sans lieu-dit.
        assert_eq!(geneanet_place("a, b, c", Some("Commune, Région, Pays"), &form), "a, b, c");
    }

    #[test]
    fn long_text_is_split() {
        let long = "mot ".repeat(120) + "fin@ici";
        let mut n = Node::with_value("NOTE", long.clone());
        n.children.push(Node::with_value("CONT", "seconde"));
        escape_text(&mut n);
        assert!(n.val().len() <= MAX_VALUE);
        assert!(n.children.iter().all(|c| c.val().len() <= MAX_VALUE + 4));
        assert_eq!(n.children.last().map(|c| (c.tag.as_str(), c.val())), Some(("CONT", "seconde")));
        // Recollé (CONC sans séparateur), le texte est le même, `@` doublé.
        let back = n.text();
        assert_eq!(back, format!("{}\nseconde", long.replace('@', "@@")));
        assert_eq!(age("1y 3m"), "1y 3m");
        assert_eq!(date("BET 1 JAN 10 BCE AND 1800", Some("x")).as_deref(), Some("BET 1 JAN 10 B.C. AND 1800"));
        assert_eq!(file_format("text/html"), "html");
    }
}
