//! Checks every date of a real file: `GEDROOTS_TEST_GED=path.ged cargo test -p gedcom --test
//! dates_file -- --nocapture`. Without the variable, the test does nothing (real files are not
//! versioned).

use gedcom::date::{Parsed, parse_input};
use gedcom::{Date, Node};

fn walk<'a>(n: &'a Node, out: &mut Vec<&'a Node>) {
    if n.tag == "DATE" {
        out.push(n);
    }
    for c in &n.children {
        walk(c, out);
    }
}

#[test]
fn every_date_roundtrips() {
    let Ok(path) = std::env::var("GEDROOTS_TEST_GED") else { return };
    let text = std::fs::read_to_string(path).unwrap();
    let ged = gedcom::Gedcom::parse(&text).unwrap();
    let mut dates = Vec::new();
    for r in &ged.records {
        walk(r, &mut dates);
    }
    let (mut other, mut invalid, mut bad) = (Vec::new(), Vec::new(), Vec::new());
    for d in &dates {
        let p = Parsed::from_gedcom(d.val(), d.child_val("PHRASE").unwrap_or(""));
        match &p.date {
            Some(Date::Other(s)) => {
                other.push(s.clone());
                continue;
            }
            Some(x) if x.validate().is_err() => invalid.push(format!("{} : {}", d.val(), x.validate().unwrap_err())),
            _ => {}
        }
        let text = p.to_input_text();
        match parse_input(&text) {
            Ok(back) if back.value() == p.value() && back.phrase == p.phrase => {}
            r => bad.push(format!("{} → “{text}” → {r:?}", d.val())),
        }
    }
    println!("{} dates ; non standard : {other:?} ; impossibles : {invalid:?}", dates.len());
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
