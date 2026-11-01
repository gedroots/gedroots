//! Convertit les `_URL` d'un fichier en liens multimédia standard :
//! `cargo run --example migrate_urls -- entree.ged sortie.ged`.

fn main() {
    let mut args = std::env::args().skip(1);
    let (src, dst) = (args.next().expect("usage : migrate_urls entree.ged sortie.ged"), args.next().expect("sortie"));
    let mut g = gedcom::load(&src).expect("lecture impossible");
    let before = gedcom::analysis::check::check(&g, &Default::default()).len();
    let n = g.migrate_url_tags();
    let after = gedcom::analysis::check::check(&g, &Default::default()).len();
    println!("{n} liens _URL convertis, {} restants · anomalies {before} → {after}", g.legacy_url_count());
    gedcom::save(&g, &dst).expect("écriture impossible");
}
