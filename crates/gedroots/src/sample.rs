//! Sample tree (Help › Open Sample Tree): a small fictional family bundled with the app, to try
//! things out without touching one's own tree. It opens in its own window, starts again from the
//! bundled files each time and cannot be saved.
//!
//! The guided tour uses it for its demos: a birth record to enter from its register
//! ([`birth_act`]) and a death record proposed as if by an AI agent ([`demo_proposal`]). The
//! records and their images are invented too.

use crate::agent::propose::Pending;
use gedcom::proposal::{NewSource, Proposal, SourceChoice};
use gedcom::{Act, ActRole, Gedcom, MediaChoice, Participant, PersonChoice, PersonField};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

const GED: &str = include_str!("../assets/sample/sample.ged");
const BIRTH_IMAGE: &[u8] = include_bytes!("../assets/sample/birth-emile-marchand.jpg");
const DEATH_IMAGE: &[u8] = include_bytes!("../assets/sample/death-jacques-marchand.jpg");

/// Root person of the sample tree.
const ROOT: &str = "I11";
/// People and records the demos use.
pub const JACQUES: &str = "I1";
const MARIE: &str = "I2";
pub const EMILE: &str = "I3";
pub const REGISTER: &str = "S1";
const BIRTH_MEDIA: &str = "O1";

/// The sample tree, as bundled, and the path it is shown under: a folder of the temporary
/// directory, written on each opening, that holds the images of its records.
pub fn open() -> Result<(Gedcom, PathBuf), String> {
    let mut ged = Gedcom::parse(GED).map_err(|e| e.to_string())?;
    ged.set_root(ROOT);
    // A new folder each time: the sample starts again from scratch, and two sample windows do not
    // share their files.
    static OPENED: AtomicUsize = AtomicUsize::new(0);
    let n = OPENED.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join("GEDRoots Sample").join(format!("{}-{n}", std::process::id()));
    let records = dir.join("Records");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&records).map_err(|e| format!("{} : {e}", records.display()))?;
    let write =
        |path: &Path, bytes: &[u8]| std::fs::write(path, bytes).map_err(|e| format!("{} : {e}", path.display()));
    write(&records.join("birth-emile-marchand.jpg"), BIRTH_IMAGE)?;
    let path = dir.join("sample.ged");
    write(&path, GED.as_bytes())?;
    Ok((ged, path))
}

fn person(role: ActRole, choice: PersonChoice, values: &[(PersonField, &str)]) -> Participant {
    let mut p = Participant::new(role);
    for (f, v) in values {
        p.set(*f, *v);
    }
    p.choice = choice;
    p
}

/// Émile's birth record, read from the register's image and entered for the tour: Émile and his
/// mother are already linked; linking his father is left to the user.
pub fn birth_act() -> Act {
    use PersonField::*;
    let mut act = Act::new(REGISTER, "BIRT");
    act.date = "FRENCH_R 30 GERM 4".into();
    act.place = "Agneaux, Manche, Normandie, France".into();
    act.page = "Record 12, view 2".into();
    act.media = MediaChoice::Existing(BIRTH_MEDIA.into());
    act.transcription = "L'an quatre de la République française, le trente germinal, par devant nous officier \
        public de la commune d'Agneaux, est comparu Jacques Marchand, laboureur, âgé de trente-deux ans, lequel \
        nous a présenté un enfant mâle né ce jour de lui et de Marie Hébert son épouse, auquel il a donné le \
        prénom d'Émile."
        .into();
    act.participants = vec![
        person(
            ActRole::Principal,
            PersonChoice::Link(EMILE.into()),
            &[(Surname, "MARCHAND"), (Given, "Émile"), (Sex, "M")],
        ),
        person(
            ActRole::Father,
            PersonChoice::Create,
            &[(Surname, "MARCHAND"), (Given, "Jacques"), (Age, "32"), (Occupation, "Ploughman")],
        ),
        person(ActRole::Mother, PersonChoice::Link(MARIE.into()), &[(Surname, "HÉBERT"), (Given, "Marie")]),
        person(
            ActRole::Witness,
            PersonChoice::Mention,
            &[(Surname, "LE MOIGNE"), (Given, "Louis"), (Age, "40"), (Occupation, "Weaver")],
        ),
    ];
    act
}

/// Jacques' death record, as an AI agent would propose it: a new source for the deaths register,
/// the image of the record, his son as declarant and a neighbour only mentioned.
pub fn demo_proposal(ged_path: &Path) -> Result<Pending, String> {
    use PersonField::*;
    // Written in the sample's Records folder and cited by a relative path, like the birth image:
    // the sample needs no media naming setting to keep its files together.
    let file = "Records/death-jacques-marchand.jpg";
    let image = ged_path.with_file_name(file);
    std::fs::write(&image, DEATH_IMAGE).map_err(|e| format!("{} : {e}", image.display()))?;
    let mut act = Act::new("", "DEAT");
    act.date = "12 MAR 1831".into();
    act.place = "Agneaux, Manche, Normandie, France".into();
    act.page = "Record 7".into();
    act.transcription = "L'an mil huit cent trente-et-un, le douze mars, à dix heures du matin, par devant nous \
        maire de la commune d'Agneaux, sont comparus Émile Marchand, tisserand, âgé de trente-quatre ans, fils \
        du défunt, et Louis Le Moigne, voisin, âgé de soixante-quinze ans, lesquels nous ont déclaré que Jacques \
        Marchand, laboureur, âgé de soixante-sept ans, époux de Marie Hébert, est décédé ce jour en son domicile."
        .into();
    act.participants = vec![
        person(
            ActRole::Principal,
            PersonChoice::Link(JACQUES.into()),
            &[(Surname, "MARCHAND"), (Given, "Jacques"), (Age, "67"), (Occupation, "Ploughman")],
        ),
        person(
            ActRole::Declarant,
            PersonChoice::Link(EMILE.into()),
            &[(Surname, "MARCHAND"), (Given, "Émile"), (Age, "34"), (Occupation, "Weaver"), (Kinship, "son")],
        ),
        person(ActRole::Neighbor, PersonChoice::Mention, &[(Surname, "LE MOIGNE"), (Given, "Louis"), (Age, "75")]),
    ];
    let source = SourceChoice::New(NewSource {
        register: "Agneaux, deaths 1831".into(),
        repository: Some("R1".into()),
        call_number: "3 E 4/2".into(),
        ..Default::default()
    });
    let mut proposal =
        Proposal { source, act, url: String::new(), conflicts: Default::default(), locations: Vec::new() };
    proposal.act.media = MediaChoice::NewFiles(vec![file.into()]);
    Ok(Pending {
        proposal,
        original: None,
        pdf_page: None,
        image: Some(image.clone()),
        preview: Some(image),
        temps: Vec::new(),
        replaces: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_tree_is_valid_and_has_nobody_living() {
        let (g, path) = open().unwrap();
        assert!(path.is_file());
        assert!(path.with_file_name("Records").join("birth-emile-marchand.jpg").is_file());
        assert_eq!(g.root().as_deref(), Some(ROOT));
        let people: Vec<String> = g.records_of(gedcom::RecordKind::Individual).filter_map(|r| r.xref.clone()).collect();
        assert!(people.len() >= 10);
        // Nobody living: everyone died, or was born more than a century ago.
        for x in people {
            let ind = g.individual(&x).unwrap();
            let dead = ind.events().iter().any(|e| matches!(e.tag(), "DEAT" | "BURI" | "CREM"));
            let old = ind.birth().and_then(|b| b.date()).and_then(|d| d.latest_year()).is_some_and(|y| y <= 1926);
            assert!(dead || old, "{x} must not be living");
        }
    }

    /// What a new user sees first passes its own structure and media checks.
    #[test]
    fn sample_tree_passes_its_own_structure_checks() {
        use gedcom::quality::{Category, category};
        let (g, _) = open().unwrap();
        let wrong: Vec<String> = gedcom::quality::run(&g)
            .into_iter()
            .filter(|i| matches!(category(i.code), Category::Structure | Category::Media | Category::Family))
            .map(|i| format!("{} {}: {}", i.code, i.xref, i.message))
            .collect();
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    #[test]
    fn birth_record_cites_the_register_for_three_people() {
        let (mut g, _) = open().unwrap();
        let mut act = birth_act();
        act.participants[1].choice = PersonChoice::Link(JACQUES.into());
        act.save(&mut g).unwrap();
        for x in [EMILE, JACQUES, MARIE] {
            assert!(gedcom::node_to_string(g.get(x).unwrap()).contains(&format!("SOUR @{REGISTER}@")), "{x} cites S1");
        }
        assert!(g.to_gedcom_string().contains("FRENCH_R 30 GERM 4"));
    }

    fn cites(g: &Gedcom, x: &str) -> bool {
        gedcom::node_to_string(g.get(x).unwrap()).contains("SOUR @")
    }

    #[test]
    fn demo_proposal_fits_the_sample_tree() {
        let (mut g, path) = open().unwrap();
        let mut p = demo_proposal(&path).unwrap();
        p.proposal.complete(&g);
        p.proposal.check(&g).unwrap();
        p.proposal.apply(&mut g).unwrap();
        assert!(g.to_gedcom_string().contains("Agneaux, deaths 1831"));
        assert!(cites(&g, JACQUES));
        assert!(g.to_gedcom_string().contains("FILE Records/death-jacques-marchand.jpg"));
    }
}
