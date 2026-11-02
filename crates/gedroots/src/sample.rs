//! Sample tree (Help › Open Sample Tree): a small fictional family bundled with the app, to try
//! things out without touching one's own tree. It opens in its own window, starts again from the
//! bundled file each time and cannot be saved.

use gedcom::Gedcom;
use std::path::PathBuf;

const GED: &str = include_str!("../assets/sample/sample.ged");

/// Root person of the sample tree.
const ROOT: &str = "I11";

/// The sample tree, as bundled, and the path it is shown under: a folder of the temporary
/// directory, rewritten on each opening, where its media files will live.
pub fn open() -> Result<(Gedcom, PathBuf), String> {
    let mut ged = Gedcom::parse(GED).map_err(|e| e.to_string())?;
    ged.set_root(ROOT);
    let dir = std::env::temp_dir().join("GEDRoots Sample");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    let path = dir.join("sample.ged");
    std::fs::write(&path, GED).map_err(|e| format!("{} : {e}", path.display()))?;
    Ok((ged, path))
}

#[cfg(test)]
mod tests {
    #[test]
    fn sample_tree_is_valid_and_has_nobody_living() {
        let (g, path) = super::open().unwrap();
        assert!(path.is_file());
        assert_eq!(g.root().as_deref(), Some(super::ROOT));
        let people: Vec<String> = g.records_of(gedcom::RecordKind::Individual).filter_map(|r| r.xref.clone()).collect();
        assert!(people.len() >= 10);
        for x in people {
            assert!(!g.is_living(&x, 2026), "{x} must not be living");
        }
    }
}
