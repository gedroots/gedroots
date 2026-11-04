//! Typesets the default book of a tree: `cargo run -p gedroots-book --example book -- tree.ged
//! out.pdf [ancestors|descendants|stored] [generations] [pages-folder]`. `stored`: the tree's first
//! book, as saved in the file, with what an agent wrote. The pages folder, when given, receives a
//! PNG of each page.

use gedroots_book::{BookSpec, Line};
use std::path::Path;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The book is written in the language of the system, like the app.
    if std::env::var("LANG").is_ok_and(|l| l.starts_with("fr")) {
        rust_i18n::set_locale("fr");
    }
    let [ged, out, rest @ ..] = args.as_slice() else {
        return Err("usage: book tree.ged out.pdf [ancestors|descendants] [generations] [pages-folder]".into());
    };
    let text = std::fs::read_to_string(ged).map_err(|e| e.to_string())?;
    let g = gedcom::document::Gedcom::parse(&text).map_err(|e| format!("{e:?}"))?;
    let root = g.root().ok_or("the tree has no root person")?;
    let mut spec = BookSpec::new(&root, "");
    match rest.first().map(String::as_str) {
        Some("descendants") => spec.line = Line::Descendants,
        Some("stored") => {
            spec = gedroots_book::spec::books(&g).into_iter().next().ok_or("the tree has no book")?;
        }
        _ => {}
    }
    if let Some(n) = rest.get(1).and_then(|n| n.parse().ok()) {
        spec.generations = n;
    }
    let name = Path::new(ged).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let started = std::time::Instant::now();
    let volumes = gedroots_book::compose_export(&g, &spec, &name, Path::new(ged).parent(), (2026, 11, 4))?;
    println!("{} volume(s) in {:?}", volumes.len(), started.elapsed());
    for (k, v) in volumes.iter().enumerate() {
        // The first volume at `out`, the others and the covers beside it.
        let path = |suffix: &str| {
            let p = Path::new(out);
            let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            p.with_file_name(format!("{stem}{suffix}.pdf"))
        };
        let tag = if volumes.len() > 1 { format!("-{}", k + 1) } else { String::new() };
        println!("{}: {} pages", path(&tag).display(), v.interior.page_count());
        std::fs::write(path(&tag), v.interior.pdf()?).map_err(|e| e.to_string())?;
        if let Some(cover) = &v.cover {
            std::fs::write(path(&format!("{tag}-cover")), cover.pdf()?).map_err(|e| e.to_string())?;
            if let Some(dir) = rest.get(2) {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                if let Some(png) = cover.page_png(0, 1800) {
                    std::fs::write(Path::new(dir).join(format!("cover{tag}.png")), png).map_err(|e| e.to_string())?;
                }
            }
        }
        if let Some(dir) = rest.get(2) {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            for i in 0..v.interior.page_count() {
                if let Some(png) = v.interior.page_png(i, 900) {
                    std::fs::write(Path::new(dir).join(format!("p{tag}{:03}.png", i + 1)), png)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    Ok(())
}
