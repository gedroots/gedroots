//! Runs the analysis tools on a file: `cargo run --example analyse -- tree.ged [@I1@]`.

use gedcom::analysis::{check, stats};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: analyse file.ged [root-xref]");
    let g = gedcom::load(&path).expect("cannot read the file");
    let t0 = std::time::Instant::now();
    let issues = check::check(&g, &check::Thresholds::default());
    println!("{} anomalies en {:?}", issues.len(), t0.elapsed());
    let mut by_code: std::collections::BTreeMap<&str, usize> = Default::default();
    for i in &issues {
        *by_code.entry(i.code).or_default() += 1;
    }
    for (c, n) in by_code {
        println!("  {n:4}  {}", check::code_label(c));
    }
    for i in issues.iter().take(15) {
        println!("  [{}] {}", i.severity.label(), i.message);
    }
    let root = args.next().map(|s| s.trim_matches('@').to_string()).or_else(|| {
        g.records_of(gedcom::RecordKind::Individual)
            .find(|r| gedcom::Individual::new(r).sosa().as_deref() == Some("1"))
            .and_then(|r| r.xref.clone())
    });
    let t0 = std::time::Instant::now();
    let s = stats::stats(&g, root.as_deref());
    println!("\nStatistiques en {:?} : {} individus, {} familles", t0.elapsed(), s.individuals, s.families);
    println!("Patronymes : {:?}", &s.surnames[..s.surnames.len().min(8)]);
    for c in &s.centuries {
        println!(
            "  {}s : {} naissances, longévité {:?} ({} décès)",
            c.start,
            c.births,
            c.mean_lifespan.map(|x| x.round()),
            c.deaths_counted
        );
    }
    for gn in &s.generations {
        println!("  G{} : {}/{} ({} distincts)", gn.number, gn.found, gn.expected, gn.distinct);
    }
    println!("Implexe : {:.1} %", s.implexe * 100.);
}
