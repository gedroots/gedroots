//! Summary of the Data Quality page for a file: `cargo run --example quality -- file.ged`.
use gedcom::quality::{self, Category};
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).expect("path of the .ged file");
    let g = gedcom::load(&path).expect("cannot read the file");
    let t = std::time::Instant::now();
    let issues = quality::run(&g);
    eprintln!("{} anomalies en {:?}", issues.len(), t.elapsed());
    let mut by: BTreeMap<(Category, &str), Vec<&gedcom::analysis::check::Issue>> = BTreeMap::new();
    for i in issues.iter().filter(|i| !i.ignored) {
        by.entry((quality::category(i.code), i.code)).or_default().push(i);
    }
    for ((c, code), v) in by {
        println!("{} / {code} : {}", c.label(), v.len());
        for i in v.iter().take(3) {
            println!("    {}", i.message);
        }
    }
}
