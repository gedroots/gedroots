//! Preparing a GEDZIP export (GEDCOM 7, chapter 4): a zip archive that contains `gedcom.ged` at its
//! root and every local file referenced by a `FILE`, under the name that `FILE` gives.
//!
//! [`pack`] does not read the disk (the application tells it which files exist): it gives a copy of
//! the file whose local `FILE`s are rewritten as paths relative to the archive, the list of files
//! to put in it with their original `FILE` value, and the missing files, whose `FILE` does not
//! change. The application writes the archive. The open file is not modified.
//!
//! Names in the archive:
//! - a relative path (`Records/birth.jpg`, `./Records\birth.jpg`) keeps its name, with `/`:
//!   unzipped, the archive is laid out like the `.ged` folder;
//! - an absolute path, `file://`, or a path that goes up (`..`) goes into `media/` under its file
//!   name, as the standard recommends;
//! - two different files never get the same name (`x (2).jpg`), and reserved names (`gedcom.ged`,
//!   `META-INF/…`) are avoided.
//!
//! Web links (`FILE https://…`) stay as they are.

use crate::Gedcom;
use std::collections::{HashMap, HashSet};

/// Name of the dataset in the archive.
pub const DATASET: &str = "gedcom.ged";

/// Extension recommended by the standard.
pub const EXTENSION: &str = "gdz";

/// A file to put in the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedFile {
    /// Name in the archive (UTF-8, `/` separators, no escaping).
    pub name: String,
    /// Original `FILE` value, to find on the disk.
    pub original: String,
    /// Media that reference this file.
    pub media: Vec<String>,
}

/// A local file that cannot be found: its `FILE` is kept as is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingFile {
    /// Valeur `FILE` d'origine.
    pub original: String,
    /// Media that reference this file.
    pub media: Vec<String>,
}

/// Copy of the file ready for the archive, files to add to it and missing files.
pub struct Packed {
    pub ged: Gedcom,
    pub files: Vec<PackedFile>,
    pub missing: Vec<MissingFile>,
}

/// Prepares the GEDZIP export of `g` (see the module). `exists` says whether the file of a `FILE`
/// value is on the disk: a missing file has no place in the archive and its `FILE` stays as is, so
/// as not to lose the original path nor point to an absent entry.
pub fn pack(g: &Gedcom, exists: &dyn Fn(&str) -> bool) -> Packed {
    let mut ged = g.clone();
    let mut files: Vec<PackedFile> = Vec::new();
    let mut missing: Vec<MissingFile> = Vec::new();
    // Original `FILE` value → name in the archive (`None`: missing).
    let mut by_original: HashMap<String, Option<usize>> = HashMap::new();
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
            let slot = *by_original.entry(original.clone()).or_insert_with(|| {
                if exists(&original) {
                    let name = free_name(archive_name(&original), &mut used);
                    files.push(PackedFile { name, original: original.clone(), media: Vec::new() });
                    Some(files.len() - 1)
                } else {
                    missing.push(MissingFile { original: original.clone(), media: Vec::new() });
                    None
                }
            });
            let cited = match slot {
                Some(i) => {
                    f.value = Some(escape(&files[i].name));
                    &mut files[i].media
                }
                None => &mut missing.iter_mut().find(|m| m.original == original).unwrap().media,
            };
            if !cited.contains(&media) {
                cited.push(media.clone());
            }
        }
    }
    ged.reindex();
    Packed { ged, files, missing }
}

/// Does the `FILE` designate a local file (and not a web address)?
pub fn is_local(file: &str) -> bool {
    let f = file.trim();
    if f.is_empty() {
        return false;
    }
    match f.split_once(':') {
        // `C:\…`: Windows drive; `file:`: local file.
        Some((scheme, _)) if scheme.len() > 1 && is_scheme(scheme) => scheme.eq_ignore_ascii_case("file"),
        _ => true,
    }
}

fn is_scheme(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|c| c.is_ascii_alphabetic())
        && c.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Name in the archive for a local `FILE` value, before deduplication.
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

/// Names that the standard reserves or advises against.
fn reserved(name: &str) -> bool {
    name == DATASET || name == "MANIFEST.MF" || name == "META-INF" || name.starts_with("META-INF/")
}

/// `name`, or `name (2)`, `name (3)`… if it is already taken.
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

/// `FILE` value for an archive name: characters that cannot appear as is in a URL are escaped
/// (`%20`…). Accented letters stay readable (valid URL string).
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
