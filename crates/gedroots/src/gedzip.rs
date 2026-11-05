//! Writing a GEDZIP archive (`.gdz`) prepared by `gedcom::gedzip::pack`, and unzipping one for
//! File › Import ([`extract`]).
//!
//! The archive is first written next to the destination (`.part`), then renamed: an interrupted
//! export never leaves an incomplete archive in place of a good one. The dataset and text files are
//! compressed; images and documents, already compressed, are stored as is (as the standard
//! advises).

use std::fs::File;
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

/// A file of the disk to put in the archive under `name`.
pub struct Entry {
    pub name: String,
    pub source: PathBuf,
}

/// Writes the archive `dest`: `gedcom.ged` (the `dataset` text), then the files.
pub fn write(dest: &Path, dataset: &str, entries: &[Entry]) -> Result<(), String> {
    let mut part = dest.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let res = write_to(&part, dataset, entries)
        .and_then(|()| std::fs::rename(&part, dest).map_err(|e| format!("{} : {e}", dest.display())));
    if res.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    res
}

fn write_to(path: &Path, dataset: &str, entries: &[Entry]) -> Result<(), String> {
    let file = File::create(path).map_err(|e| format!("{} : {e}", path.display()))?;
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let err = |e: zip::result::ZipError| e.to_string();
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(gedcom::gedzip::DATASET, deflated.last_modified_time(date_time(SystemTime::now()))).map_err(err)?;
    io::Write::write_all(&mut zip, dataset.as_bytes()).map_err(|e| e.to_string())?;
    for e in entries {
        let mut src = File::open(&e.source).map_err(|err| format!("{} : {err}", e.source.display()))?;
        let meta = src.metadata().map_err(|err| format!("{} : {err}", e.source.display()))?;
        let method = if gedcom::mime_for(&e.name).starts_with("text/") {
            CompressionMethod::Deflated
        } else {
            CompressionMethod::Stored
        };
        let mut opts = SimpleFileOptions::default()
            .compression_method(method)
            .large_file(meta.len() >= u32::MAX as u64)
            .unix_permissions(0o644);
        if let Ok(t) = meta.modified() {
            opts = opts.last_modified_time(date_time(t));
        }
        zip.start_file(e.name.as_str(), opts).map_err(err)?;
        io::copy(&mut src, &mut zip).map_err(|err| format!("{} : {err}", e.source.display()))?;
    }
    let out = zip.finish().map_err(err)?;
    out.into_inner().map_err(|e| e.error().to_string())?.sync_all().map_err(|e| e.to_string())
}

/// Zip date (UTC time, to within 2 s); dates outside 1980–2107 become 1980.
fn date_time(t: SystemTime) -> DateTime {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Jours depuis 1970 → date civile (algorithme de H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as u16;
    let (h, m, s) = ((rem / 3600) as u8, (rem / 60 % 60) as u8, (rem % 60) as u8);
    DateTime::from_date_and_time(year, month, day, h, m, s).unwrap_or_default()
}

/// Folder next to `archive` to unzip it into: its name without the extension, numbered if a
/// folder of that name already holds something (`tree 2`).
pub fn extraction_folder(archive: &Path) -> PathBuf {
    let dir = archive.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = archive.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "gedzip".into());
    let free = |p: &Path| std::fs::read_dir(p).map(|mut d| d.next().is_none()).unwrap_or(!p.exists());
    (1..).map(|n| dir.join(if n == 1 { stem.clone() } else { format!("{stem} {n}") })).find(|p| free(p)).unwrap()
}

/// Unzips the GEDZIP `archive` into `dest`: returns the text of its `gedcom.ged`, which is not
/// written to the disk, and the number of files unzipped. Entries whose name would leave `dest`
/// (`../`, absolute) and `META-INF/` are skipped.
pub fn extract(archive: &Path, dest: &Path) -> Result<(String, usize), String> {
    let at = |p: &Path, e: &dyn std::fmt::Display| format!("{} : {e}", p.display());
    let file = File::open(archive).map_err(|e| at(archive, &e))?;
    let mut zip = zip::ZipArchive::new(io::BufReader::new(file)).map_err(|e| at(archive, &e))?;
    let mut dataset = None;
    let mut count = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| at(archive, &e))?;
        if entry.is_dir() {
            continue;
        }
        let Some(name) = entry.enclosed_name() else { continue };
        if name == Path::new(gedcom::gedzip::DATASET) {
            let mut text = String::new();
            io::Read::read_to_string(&mut entry, &mut text).map_err(|e| at(archive, &e))?;
            dataset = Some(text);
            continue;
        }
        if name.starts_with("META-INF") {
            continue;
        }
        let path = dest.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| at(parent, &e))?;
        }
        let mut out = File::create(&path).map_err(|e| at(&path, &e))?;
        io::copy(&mut entry, &mut out).map_err(|e| at(&path, &e))?;
        count += 1;
    }
    let dataset = dataset.ok_or_else(|| crate::t!("import.no_dataset").into_owned())?;
    Ok((dataset, count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::time::Duration;

    #[test]
    fn writes_dataset_and_files() {
        let dir = std::env::temp_dir().join(format!("gedroots-gedzip-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Actes")).unwrap();
        let photo = dir.join("Actes").join("naissance Jean.jpg");
        std::fs::write(&photo, [0xFF, 0xD8, 0xFF, 1, 2, 3]).unwrap();
        let dest = dir.join("export.gdz");
        let entries = [Entry { name: "Actes/naissance Jean.jpg".into(), source: photo }];
        write(&dest, "0 HEAD\n0 TRLR\n", &entries).unwrap();
        assert!(!dir.join("export.gdz.part").exists());

        let mut zip = zip::ZipArchive::new(File::open(&dest).unwrap()).unwrap();
        assert_eq!(zip.len(), 2);
        let mut ged = String::new();
        zip.by_name("gedcom.ged").unwrap().read_to_string(&mut ged).unwrap();
        assert_eq!(ged, "0 HEAD\n0 TRLR\n");
        let mut f = zip.by_name("Actes/naissance Jean.jpg").unwrap();
        assert_eq!(f.compression(), CompressionMethod::Stored);
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, [0xFF, 0xD8, 0xFF, 1, 2, 3]);
        drop(f);

        // A missing file makes the export fail without leaving anything behind.
        let gone = [Entry { name: "x.jpg".into(), source: dir.join("absent.jpg") }];
        assert!(write(&dir.join("autre.gdz"), "", &gone).is_err());
        assert!(!dir.join("autre.gdz").exists() && !dir.join("autre.gdz.part").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn zip_dates() {
        let t = UNIX_EPOCH + Duration::from_secs(1_793_612_345); // 2026-11-02 09:39:05 UTC
        let d = date_time(t);
        assert_eq!((d.year(), d.month(), d.day(), d.hour(), d.minute()), (2026, 11, 2, 9, 39));
        assert_eq!(date_time(UNIX_EPOCH).year(), 1980);
    }

    #[test]
    fn extracts_files_but_not_the_dataset() {
        let dir = std::env::temp_dir().join(format!("gedroots-unzip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let photo = dir.join("p.jpg");
        std::fs::write(&photo, [1, 2, 3]).unwrap();
        let archive = dir.join("Family tree.gdz");
        let entries = [Entry { name: "Actes/naissance Jean.jpg".into(), source: photo }];
        write(&archive, "0 HEAD\n0 TRLR\n", &entries).unwrap();
        std::fs::create_dir_all(dir.join("Family tree")).unwrap();
        std::fs::write(dir.join("Family tree").join("x"), "").unwrap();
        let dest = extraction_folder(&archive);
        assert_eq!(dest, dir.join("Family tree 2"), "a folder that holds something is not reused");
        let (text, n) = extract(&archive, &dest).unwrap();
        assert_eq!((text.as_str(), n), ("0 HEAD\n0 TRLR\n", 1));
        assert_eq!(std::fs::read(dest.join("Actes").join("naissance Jean.jpg")).unwrap(), [1, 2, 3]);
        assert!(!dest.join("gedcom.ged").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
