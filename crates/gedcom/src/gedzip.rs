//! Préparation d'un export GEDZIP (GEDCOM 7, chapitre 4) : une archive zip qui
//! contient `gedcom.ged` à la racine et chaque fichier local cité par un `FILE`,
//! sous le nom que donne ce `FILE`.
//!
//! [`pack`] ne lit pas le disque : il donne une copie du fichier dont les `FILE`
//! locaux sont réécrits en chemins relatifs à l'archive, et la liste des fichiers
//! à y mettre avec leur valeur `FILE` d'origine. L'application retrouve ces
//! fichiers sur le disque et écrit l'archive. Le fichier ouvert n'est pas modifié.
//!
//! Noms dans l'archive :
//! - un chemin relatif (`Actes/naissance.jpg`, `./Actes\naissance.jpg`) garde
//!   son nom, avec des `/` : décompressée, l'archive se range comme le dossier
//!   du `.ged` ;
//! - un chemin absolu, `file://`, ou qui remonte (`..`) va dans `media/` sous son
//!   nom de fichier, comme le recommande la norme ;
//! - deux fichiers différents ne prennent jamais le même nom (`x (2).jpg`), et
//!   les noms réservés (`gedcom.ged`, `META-INF/…`) sont évités.
//!
//! Les liens web (`FILE https://…`) restent tels quels.

use crate::Gedcom;
use std::collections::{HashMap, HashSet};

/// Nom du jeu de données dans l'archive.
pub const DATASET: &str = "gedcom.ged";

/// Extension recommandée par la norme.
pub const EXTENSION: &str = "gdz";

/// Un fichier à mettre dans l'archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedFile {
    /// Nom dans l'archive (UTF-8, séparateurs `/`, sans échappement).
    pub name: String,
    /// Valeur `FILE` d'origine, à retrouver sur le disque.
    pub original: String,
    /// Médias qui citent ce fichier.
    pub media: Vec<String>,
}

/// Copie du fichier prête pour l'archive, et fichiers à y joindre.
pub struct Packed {
    pub ged: Gedcom,
    pub files: Vec<PackedFile>,
}

/// Prépare l'export GEDZIP de `g` (voir le module).
pub fn pack(g: &Gedcom) -> Packed {
    let mut ged = g.clone();
    let mut files: Vec<PackedFile> = Vec::new();
    // Valeur `FILE` d'origine → position dans `files`.
    let mut by_original: HashMap<String, usize> = HashMap::new();
    let mut used: HashSet<String> = HashSet::new();
    for r in ged.records.iter_mut().filter(|r| r.tag == "OBJE") {
        if !r.children.iter().any(|c| c.tag == "FILE" && is_local(c.val())) {
            continue;
        }
        let media = r.xref.clone().unwrap_or_default();
        let r = std::sync::Arc::make_mut(r);
        for f in r.children.iter_mut().filter(|c| c.tag == "FILE") {
            let original = f.val().trim().to_string();
            if !is_local(&original) {
                continue;
            }
            let i = *by_original.entry(original.clone()).or_insert_with(|| {
                let name = free_name(archive_name(&original), &mut used);
                files.push(PackedFile { name, original: original.clone(), media: Vec::new() });
                files.len() - 1
            });
            if !files[i].media.contains(&media) {
                files[i].media.push(media.clone());
            }
            f.value = Some(escape(&files[i].name));
        }
    }
    ged.reindex();
    Packed { ged, files }
}

/// Le `FILE` désigne-t-il un fichier local (et non une adresse web) ?
pub fn is_local(file: &str) -> bool {
    let f = file.trim();
    if f.is_empty() {
        return false;
    }
    match f.split_once(':') {
        // `C:\…` : lecteur Windows ; `file:` : fichier local.
        Some((scheme, _)) if scheme.len() > 1 && is_scheme(scheme) => scheme.eq_ignore_ascii_case("file"),
        _ => true,
    }
}

fn is_scheme(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|c| c.is_ascii_alphabetic())
        && c.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Nom dans l'archive pour une valeur `FILE` locale, avant dédoublonnage.
fn archive_name(file: &str) -> String {
    let f = file.trim();
    let path = match f.get(..5) {
        Some(p) if p.eq_ignore_ascii_case("file:") => {
            // `file:///Users/…`, `file://localhost/…`, `file:/…` : toujours absolu.
            let rest = &f[5..];
            let rest = rest.strip_prefix("//").map(|r| r.find('/').map_or("", |i| &r[i..])).unwrap_or(rest);
            return format!("media/{}", file_name(&rest.replace('\\', "/")));
        }
        _ => f.replace('\\', "/"),
    };
    let drive = path.as_bytes().get(1) == Some(&b':') && path.as_bytes()[0].is_ascii_alphabetic();
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    if path.starts_with('/') || drive || parts.contains(&"..") || parts.is_empty() {
        return format!("media/{}", file_name(&path));
    }
    let name = parts.join("/");
    if reserved(&name) { format!("media/{}", file_name(&name)) } else { name }
}

fn file_name(path: &str) -> String {
    let n = path.rsplit('/').find(|p| !p.is_empty() && *p != "." && *p != "..").unwrap_or("fichier");
    if reserved(n) { format!("_{n}") } else { n.to_string() }
}

/// Noms que la norme réserve ou déconseille.
fn reserved(name: &str) -> bool {
    name == DATASET || name == "MANIFEST.MF" || name == "META-INF" || name.starts_with("META-INF/")
}

/// `name`, ou `name (2)`, `name (3)`… s'il est déjà pris.
fn free_name(name: String, used: &mut HashSet<String>) -> String {
    let name = if used.contains(&name) {
        let (dir, file) = name.rsplit_once('/').map_or(("", name.as_str()), |(d, f)| (d, f));
        let (stem, ext) = match file.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
            _ => (file, String::new()),
        };
        let dir = if dir.is_empty() { String::new() } else { format!("{dir}/") };
        (2..).map(|n| format!("{dir}{stem} ({n}){ext}")).find(|c| !used.contains(c)).unwrap()
    } else {
        name
    };
    used.insert(name.clone());
    name
}

/// Valeur `FILE` pour un nom d'archive : les caractères qui ne peuvent pas
/// figurer tels quels dans une URL sont échappés (`%20`…). Les lettres
/// accentuées restent lisibles (chaîne d'URL valide).
pub fn escape(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_control()
            || matches!(c, ' ' | '"' | '#' | '%' | '<' | '>' | '?' | '[' | '\\' | ']' | '^' | '`' | '{' | '|' | '}')
        {
            let mut buf = [0; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}
