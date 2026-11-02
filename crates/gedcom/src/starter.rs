//! First generations of a new tree (File › New Tree): the root person, their parents and their
//! grandparents, entered in one go. Empty slots create nobody.

use crate::document::Gedcom;
use crate::relations::NewEvent;

/// A person of the starting tree. Dates are GEDCOM values with their phrase, as
/// [`crate::date::parse_input`] returns them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StarterPerson {
    pub given: String,
    pub surname: String,
    /// `M`, `F`, `X` or `U`; ancestors take the sex of their slot.
    pub sex: String,
    pub birth: (String, String),
    pub birth_place: String,
    pub death: (String, String),
}

impl StarterPerson {
    /// Nothing entered: the slot creates nobody.
    pub fn is_empty(&self) -> bool {
        [&self.given, &self.surname, &self.birth.0, &self.birth.1, &self.birth_place, &self.death.0, &self.death.1]
            .iter()
            .all(|s| s.trim().is_empty())
    }
}

/// Slots of the starting tree, in the order of [`Starter::people`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Root,
    Father,
    Mother,
    PaternalGrandfather,
    PaternalGrandmother,
    MaternalGrandfather,
    MaternalGrandmother,
}

impl Slot {
    pub const ALL: [Slot; 7] = [
        Slot::Root,
        Slot::Father,
        Slot::Mother,
        Slot::PaternalGrandfather,
        Slot::PaternalGrandmother,
        Slot::MaternalGrandfather,
        Slot::MaternalGrandmother,
    ];

    /// Sex implied by the slot (`None` for the root, who chooses).
    pub fn sex(self) -> Option<&'static str> {
        match self {
            Slot::Root => None,
            Slot::Father | Slot::PaternalGrandfather | Slot::MaternalGrandfather => Some("M"),
            _ => Some("F"),
        }
    }

    /// Child of this slot in the starting tree.
    pub fn child(self) -> Option<Slot> {
        match self {
            Slot::Root => None,
            Slot::Father | Slot::Mother => Some(Slot::Root),
            Slot::PaternalGrandfather | Slot::PaternalGrandmother => Some(Slot::Father),
            Slot::MaternalGrandfather | Slot::MaternalGrandmother => Some(Slot::Mother),
        }
    }

    /// Person whose surname this slot inherits when left empty: a father takes his child's.
    pub fn surname_from(self) -> Option<Slot> {
        match self {
            Slot::Father | Slot::PaternalGrandfather | Slot::MaternalGrandfather => self.child(),
            _ => None,
        }
    }
}

/// The people entered, indexed by [`Slot`].
#[derive(Debug, Clone, Default)]
pub struct Starter {
    pub people: [StarterPerson; 7],
}

impl Starter {
    pub fn get(&self, slot: Slot) -> &StarterPerson {
        &self.people[slot as usize]
    }

    /// Surname suggested for an empty surname field: the one of the person it comes from, itself
    /// suggested if empty (a paternal grandfather gets the root's surname through the father).
    pub fn suggested_surname(&self, slot: Slot) -> String {
        let Some(from) = slot.surname_from() else { return String::new() };
        let own = self.get(from).surname.trim();
        if own.is_empty() { self.suggested_surname(from) } else { own.to_string() }
    }

    /// Is the slot created? A parent is created when they, or one of their own parents, were
    /// entered, so that the grandparents stay linked to the root.
    fn needed(&self, slot: Slot) -> bool {
        !self.get(slot).is_empty() || Slot::ALL.iter().any(|s| s.child() == Some(slot) && self.needed(*s))
    }
}

impl Gedcom {
    /// Creates the people of the starting tree, their families and their birth and death, makes
    /// the first one the root individual and returns them.
    pub fn seed(&mut self, starter: &Starter) -> Result<String, String> {
        let mut xrefs: [Option<String>; 7] = Default::default();
        for slot in Slot::ALL {
            if slot != Slot::Root && !starter.needed(slot) {
                continue;
            }
            let p = starter.get(slot);
            let surname = match p.surname.trim() {
                "" => starter.suggested_surname(slot),
                s => s.to_string(),
            };
            let sex = slot.sex().unwrap_or(match p.sex.as_str() {
                s @ ("M" | "F" | "X") => s,
                _ => "U",
            });
            let x = self.create_individual(p.given.trim(), &surname, sex);
            for (tag, (date, phrase), place) in [("BIRT", &p.birth, p.birth_place.as_str()), ("DEAT", &p.death, "")] {
                if date.is_empty() && phrase.is_empty() && place.trim().is_empty() {
                    continue;
                }
                let ev = NewEvent {
                    tag: tag.into(),
                    date: date.clone(),
                    date_phrase: phrase.clone(),
                    place: place.trim().into(),
                    ..Default::default()
                };
                self.add_event(&x, &ev)?;
            }
            xrefs[slot as usize] = Some(x);
        }
        // One family per couple of parents, created when at least one of them is.
        for (child, father, mother) in [
            (Slot::Root, Slot::Father, Slot::Mother),
            (Slot::Father, Slot::PaternalGrandfather, Slot::PaternalGrandmother),
            (Slot::Mother, Slot::MaternalGrandfather, Slot::MaternalGrandmother),
        ] {
            let (Some(c), f, m) =
                (&xrefs[child as usize], xrefs[father as usize].as_ref(), xrefs[mother as usize].as_ref())
            else {
                continue;
            };
            if f.is_none() && m.is_none() {
                continue;
            }
            let fam = self.create_family();
            if let Some(f) = f {
                self.set_parent_slot(&fam, f, Some("HUSB"))?;
            }
            if let Some(m) = m {
                self.set_parent_slot(&fam, m, Some("WIFE"))?;
            }
            self.add_child_to_family(&fam, c)?;
        }
        let root = xrefs[0].clone().unwrap_or_default();
        self.set_root(&root);
        Ok(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(given: &str, surname: &str) -> StarterPerson {
        StarterPerson { given: given.into(), surname: surname.into(), ..Default::default() }
    }

    #[test]
    fn root_alone_becomes_the_root() {
        let mut g = Gedcom::new();
        let mut s = Starter::default();
        s.people[0] = StarterPerson {
            sex: "F".into(),
            birth: ("14 MAR 1988".into(), String::new()),
            birth_place: "Rouen".into(),
            ..person("Claire", "MARCHAND")
        };
        let root = g.seed(&s).unwrap();
        assert_eq!(g.root().as_deref(), Some(root.as_str()));
        let i = g.individual(&root).unwrap();
        assert_eq!(i.sex(), "F");
        assert_eq!(i.surname(), "MARCHAND");
        let text = g.to_gedcom_string();
        assert!(text.contains("1 BIRT\n2 DATE 14 MAR 1988\n2 PLAC Rouen"), "{text}");
        assert!(!text.contains("DEAT"));
        assert!(!text.contains("FAM"), "no family without parents");
    }

    #[test]
    fn three_generations_and_inherited_surnames() {
        let mut g = Gedcom::new();
        let mut s = Starter::default();
        s.people[Slot::Root as usize] = person("Claire", "MARCHAND");
        s.people[Slot::Father as usize] = person("Paul", "");
        s.people[Slot::Mother as usize] = person("Sophie", "LEROY");
        s.people[Slot::PaternalGrandfather as usize] =
            StarterPerson { death: ("2004".into(), String::new()), ..person("Émile", "") };
        s.people[Slot::MaternalGrandmother as usize] = person("Jeanne", "");
        assert_eq!(s.suggested_surname(Slot::PaternalGrandfather), "MARCHAND");
        assert_eq!(s.suggested_surname(Slot::MaternalGrandfather), "LEROY");
        assert_eq!(s.suggested_surname(Slot::MaternalGrandmother), "");
        let root = g.seed(&s).unwrap();
        let (f, m) = g.parents(&root);
        let (f, m) = (f.unwrap(), m.unwrap());
        assert_eq!(f.surname(), "MARCHAND");
        assert_eq!(m.surname(), "LEROY");
        let (gf, gm) = g.parents(f.xref());
        assert_eq!(gf.unwrap().surname(), "MARCHAND");
        assert!(gm.is_none(), "an empty slot creates nobody");
        let (mgf, mgm) = g.parents(m.xref());
        assert!(mgf.is_none());
        let mgm = mgm.unwrap();
        assert_eq!((mgm.sex(), mgm.surname().as_str()), ("F", ""));
        assert_eq!(g.records_of(crate::document::RecordKind::Individual).count(), 5);
    }

    #[test]
    fn missing_parent_is_created_for_a_grandparent() {
        let mut g = Gedcom::new();
        let mut s = Starter::default();
        s.people[Slot::Root as usize] = person("Claire", "MARCHAND");
        s.people[Slot::PaternalGrandmother as usize] = person("Louise", "DURAND");
        let root = g.seed(&s).unwrap();
        let (f, m) = g.parents(&root);
        assert!(m.is_none());
        let f = f.unwrap();
        assert_eq!(f.surname(), "MARCHAND");
        assert!(g.parents(f.xref()).1.is_some());
    }
}
