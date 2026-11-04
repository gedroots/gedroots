//! Typesets the default book of a tree: `cargo run -p gedroots-book --example book -- tree.ged
//! out.pdf [ancestors|descendants] [generations] [pages-folder]`. The pages folder, when given,
//! receives a PNG of each page.

use gedroots_book::{BookSpec, Line};
use std::path::Path;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [ged, out, rest @ ..] = args.as_slice() else {
        return Err("usage: book tree.ged out.pdf [ancestors|descendants] [generations] [pages-folder]".into());
    };
    let text = std::fs::read_to_string(ged).map_err(|e| e.to_string())?;
    let g = gedcom::document::Gedcom::parse(&text).map_err(|e| format!("{e:?}"))?;
    let root = g.root().ok_or("the tree has no root person")?;
    let mut spec = BookSpec::new(&root, "");
    if rest.first().map(String::as_str) == Some("descendants") {
        spec.line = Line::Descendants;
    }
    if let Some(n) = rest.get(1).and_then(|n| n.parse().ok()) {
        spec.generations = n;
    }
    let name = Path::new(ged).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let started = std::time::Instant::now();
    let book = gedroots_book::compose(&g, &spec, &name, Path::new(ged).parent(), (2026, 11, 4))?;
    println!("{} pages in {:?}", book.page_count(), started.elapsed());
    std::fs::write(out, book.pdf()?).map_err(|e| e.to_string())?;
    if let Some(dir) = rest.get(2) {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        for i in 0..book.page_count() {
            if let Some(png) = book.page_png(i, 900) {
                std::fs::write(Path::new(dir).join(format!("p{:03}.png", i + 1)), png).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}
