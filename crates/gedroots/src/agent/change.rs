//! `propose_change`: a change for a Data Quality issue (fix, edit, hide), or a correction read in a
//! document without an issue (a misread given name, a wrong date), turned into [`gedcom::change`]
//! types. GEDRoots checks it against the file before queueing it in To Review.

use gedcom::change::{Action, Edit, Evidence};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChangeParams {
    /// Tree to work on, as `list_trees` names it, when several are open (default: the one in
    /// front in GEDRoots).
    pub tree: Option<String>,
    /// Issue id, as `quality_issues` gives it. Leave it out for a correction you read in a
    /// document (a given name misread in an earlier proposal, a wrong date): `edit` only, with
    /// `reason` and `evidence`.
    pub issue: Option<String>,
    /// `fix`: the Quality page's own fix (only when the issue has `automatic_fix`). `edit`: the
    /// edits in `edits`. `hide`: the issue is right (e.g. a marriage really celebrated very young),
    /// and the reason says why; for a possible duplicate, they are two different people or places.
    /// `merge`: a possible duplicate is one person (or one place), the two become one.
    pub action: String,
    /// For `edit`: what to change, among the issue's `edits`.
    pub edits: Option<Vec<EditParams>>,
    /// For `merge`: for each fact of the issue's `merge.conflicts`, the person (`keep` or `other`)
    /// whose value is right, e.g. `{"BIRT": "I12"}`; for two places, the place whose level is
    /// right, e.g. `{"Region": ", Leudon, 77320, Seine-et-Marne, Île-de-France, France"}`.
    pub choices: Option<std::collections::BTreeMap<String, String>>,
    /// Why: what you read, in a sentence the user can check. Required for `edit` and `hide`.
    pub reason: Option<String>,
    /// What supports the change.
    pub evidence: Option<EvidenceParams>,
}

/// One edit. `kind` says which, and which fields it needs:
/// `set_event` (record, event, value, date, place and/or address), `add_event` (record, tag,
/// value, date, place, address), `rename_place` (place, new_place), `delete_event` (record, event),
/// `set_sex` (record, sex), `set_coordinates` (place, latitude, longitude, osm),
/// `set_repository` (record: a source; repository_id or repository, call_number),
/// `set_media_file` (file), `cite_source` (record, event, source, page, quality),
/// `set_value` (value), `delete_structure` (nothing: the structure the issue designates),
/// `set_name` (record, given and/or surname, name), `add_name` (record, given and/or surname,
/// name_type), `set_age` (record, event, age, person on a family event), `link_relative` and
/// `unlink_relative` (record, relation, person), `set_pedigree` (record, pedigree, person: a
/// parent), `add_note` (record, event or not, text), `set_note` (record, event or not, note, text),
/// `add_associate` (record, event, person, role, phrase), `remove_associate` (record, event,
/// person), `attach_media` (record, event or not, media or file), `detach_media` (record, event or
/// not, media). Without an issue: every edit but set_media_file, set_value and delete_structure.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditParams {
    pub kind: String,
    /// Person or family id.
    pub record: Option<String>,
    /// Position of the event in its record (`index` in `person_sheet`).
    pub event: Option<usize>,
    /// GEDCOM date (`12 MAR 1869`, `ABT 1850`); empty to remove it.
    pub date: Option<String>,
    /// Place as the tree names it (the municipality and its jurisdictions, never a street).
    pub place: Option<String>,
    /// Street address within the place (`ADDR`: number, street, building); empty to remove it.
    pub address: Option<String>,
    /// For `rename_place`: the place's new name, everywhere it is used, in the file's format.
    pub new_place: Option<String>,
    /// `M`, `F`, `X` or `U`.
    pub sex: Option<String>,
    /// Decimal degrees.
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// For `set_coordinates`: the `osm` of the `locate_place` candidate (`relation/415228`).
    pub osm: Option<String>,
    /// For `set_repository`: an existing repository (`search_sources`).
    pub repository_id: Option<String>,
    /// For `set_repository`, when the tree has no repository for that archive: the one to create.
    pub repository: Option<super::propose::RepositoryParams>,
    /// For `set_repository`: the call number of the register at the repository (`CALN`).
    pub call_number: Option<String>,
    /// Path of the media file, relative to the tree's folder.
    pub file: Option<String>,
    /// Existing source id (`search_sources`).
    pub source: Option<String>,
    /// Reference in the source (page, act number, view).
    pub page: Option<String>,
    /// For `cite_source`: how much the source supports the event, as GEDCOM 7 `QUAY` (3 primary
    /// evidence, 2 secondary, 1 questionable, 0 unreliable or estimated).
    pub quality: Option<u8>,
    /// For `set_event` and `add_event`: the event's value (an occupation, a custom event's type);
    /// for `set_value`, the structure's new value.
    pub value: Option<String>,
    /// For `add_event`: GEDCOM tag of the event or attribute (`OCCU`, `RESI`, `BIRT`, `EVEN`…).
    pub tag: Option<String>,
    /// Given names, as the document writes them.
    pub given: Option<String>,
    pub surname: Option<String>,
    /// For `set_name`: position of the name among the person's names (0, the main one, when left
    /// out).
    pub name: Option<usize>,
    /// For `add_name`: `AKA`, `BIRTH`, `MARRIED`, `MAIDEN`, `IMMIGRANT`, `PROFESSIONAL` or `OTHER`.
    pub name_type: Option<String>,
    /// For `set_event`: the cause (`CAUS`, of a death…); empty to remove it.
    pub cause: Option<String>,
    /// For `set_age`: the age at the event (`42y`, `6m`, `3d`); empty to remove it.
    pub age: Option<String>,
    /// The other person: the relative for `link_relative` and `unlink_relative`, one of the
    /// parents for `set_pedigree`, the associate for `add_associate` and `remove_associate`, whose
    /// age it is for `set_age` on a family event.
    pub person: Option<String>,
    /// For `link_relative` and `unlink_relative`: `father`, `mother`, `spouse`, `child` or
    /// `sibling`, what `person` is (or is not) to `record`.
    pub relation: Option<String>,
    /// For `set_pedigree`: `BIRTH`, `ADOPTED`, `FOSTER`, `SEALING` or `OTHER`.
    pub pedigree: Option<String>,
    /// For `add_note` and `set_note`: the note's text; empty in `set_note` to remove it. A note
    /// comments on facts already in the tree; an act you found is proposed with `propose_act`.
    pub text: Option<String>,
    /// For `set_note`: position of the note among the notes of the record or event (`notes` in
    /// `person_sheet`, from 0).
    pub note: Option<usize>,
    /// For `add_associate`: `WITN`, `GODP`, `OFFICIATOR`, `CLERGY`, `NGHBR`, `FRIEND`, `PARENT` or
    /// `OTHER`.
    pub role: Option<String>,
    /// For `add_associate`: the rest of the role, as the act says it (“godmother”, “uncle of the
    /// bride”); required for `OTHER`.
    pub phrase: Option<String>,
    /// For `attach_media` and `detach_media`: an existing media id (`media` in `person_sheet`).
    pub media: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EvidenceParams {
    /// Source of the tree that supports it.
    pub source: Option<String>,
    /// Reference in that source.
    pub page: Option<String>,
    /// Web page of the document (the archive's viewer).
    pub url: Option<String>,
    /// Short quote of the document.
    pub quote: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ChangeProposed {
    /// Always "waiting for review": the user compares and saves it in GEDRoots.
    pub status: String,
    /// Proposals waiting for review in GEDRoots.
    pub waiting: usize,
    /// Issues the change would create, to fix as well or to mention to the user.
    pub new_issues: Vec<String>,
}

/// What the app needs to build and check the change.
#[derive(Debug, Clone)]
pub struct ChangeRequest {
    /// `None`: a correction without an issue.
    pub issue_id: Option<String>,
    pub action: Action,
    pub reason: String,
    pub evidence: Evidence,
}

fn id(v: &Option<String>) -> Option<String> {
    v.as_deref().map(|s| s.trim().trim_matches('@').to_string()).filter(|s| !s.is_empty())
}

fn need<T>(v: Option<T>, kind: &str, field: &str) -> Result<T, String> {
    v.ok_or_else(|| format!("`{kind}` needs `{field}`"))
}

fn edit(p: &EditParams) -> Result<Edit, String> {
    let k = p.kind.trim();
    Ok(match k {
        "set_event" => {
            if p.value.is_none() && p.date.is_none() && p.place.is_none() && p.address.is_none() && p.cause.is_none() {
                return Err("`set_event` needs `value`, `date`, `place`, `address` and/or `cause`".into());
            }
            Edit::SetEvent {
                record: need(id(&p.record), k, "record")?,
                index: need(p.event, k, "event")?,
                value: p.value.clone(),
                date: p.date.clone(),
                place: p.place.clone(),
                address: p.address.clone(),
                cause: p.cause.clone(),
            }
        }
        "add_event" => Edit::AddEvent {
            record: need(id(&p.record), k, "record")?,
            tag: need(p.tag.clone(), k, "tag")?,
            value: p.value.clone().unwrap_or_default(),
            date: p.date.clone().unwrap_or_default(),
            place: p.place.clone().unwrap_or_default(),
            address: p.address.clone().unwrap_or_default(),
        },
        "rename_place" => Edit::RenamePlace {
            place: need(p.place.clone(), k, "place")?,
            name: need(p.new_place.clone(), k, "new_place")?,
        },
        "delete_event" => {
            Edit::DeleteEvent { record: need(id(&p.record), k, "record")?, index: need(p.event, k, "event")? }
        }
        "set_sex" => Edit::SetSex { person: need(id(&p.record), k, "record")?, sex: need(p.sex.clone(), k, "sex")? },
        "set_coordinates" => Edit::SetCoordinates {
            place: need(p.place.clone(), k, "place")?,
            latitude: need(p.latitude, k, "latitude")?,
            longitude: need(p.longitude, k, "longitude")?,
            exid: match p.osm.as_deref().map(str::trim).filter(|o| !o.is_empty()) {
                Some(osm) => Some(super::propose::osm_exid(osm)?),
                None => None,
            },
        },
        "set_repository" => Edit::SetRepository {
            source: need(id(&p.record), k, "record")?,
            repository: id(&p.repository_id),
            new: p.repository.as_ref().map(|r| gedcom::proposal::NewRepository {
                name: r.name.trim().to_string(),
                address: r.address.as_deref().unwrap_or_default().trim().to_string(),
                www: r.website.as_deref().unwrap_or_default().trim().to_string(),
            }),
            call_number: p.call_number.clone(),
        },
        "set_media_file" => Edit::SetMediaFile { file: need(p.file.clone(), k, "file")? },
        "cite_source" => Edit::CiteSource {
            record: need(id(&p.record), k, "record")?,
            index: need(p.event, k, "event")?,
            source: need(id(&p.source), k, "source")?,
            page: p.page.clone().unwrap_or_default(),
            quality: super::propose::quality(p.quality, "quality")?,
        },
        "set_value" => Edit::SetValue { value: need(p.value.clone(), k, "value")? },
        "delete_structure" => Edit::DeleteStructure,
        "set_name" => Edit::SetName {
            person: need(id(&p.record), k, "record")?,
            index: p.name.unwrap_or(0),
            given: p.given.clone(),
            surname: p.surname.clone(),
        },
        "add_name" => Edit::AddName {
            person: need(id(&p.record), k, "record")?,
            given: p.given.clone().unwrap_or_default(),
            surname: p.surname.clone().unwrap_or_default(),
            kind: p.name_type.clone().unwrap_or_default(),
        },
        "set_age" => Edit::SetAge {
            record: need(id(&p.record), k, "record")?,
            index: need(p.event, k, "event")?,
            spouse: id(&p.person),
            age: need(p.age.clone(), k, "age")?,
        },
        "link_relative" => Edit::LinkRelative {
            person: need(id(&p.record), k, "record")?,
            relation: relation(need(p.relation.as_deref(), k, "relation")?)?,
            other: need(id(&p.person), k, "person")?,
        },
        "unlink_relative" => Edit::UnlinkRelative {
            person: need(id(&p.record), k, "record")?,
            relation: relation(need(p.relation.as_deref(), k, "relation")?)?,
            other: need(id(&p.person), k, "person")?,
        },
        "set_pedigree" => Edit::SetPedigree {
            person: need(id(&p.record), k, "record")?,
            parent: id(&p.person),
            pedigree: need(p.pedigree.clone(), k, "pedigree")?,
        },
        "add_note" => Edit::AddNote {
            record: need(id(&p.record), k, "record")?,
            event: p.event,
            text: need(p.text.clone(), k, "text")?,
        },
        "set_note" => Edit::SetNote {
            record: need(id(&p.record), k, "record")?,
            event: p.event,
            note: need(p.note, k, "note")?,
            text: need(p.text.clone(), k, "text")?,
        },
        "add_associate" => Edit::AddAssociate {
            record: need(id(&p.record), k, "record")?,
            index: need(p.event, k, "event")?,
            person: need(id(&p.person), k, "person")?,
            role: need(p.role.clone(), k, "role")?,
            phrase: p.phrase.clone().unwrap_or_default(),
        },
        "remove_associate" => Edit::RemoveAssociate {
            record: need(id(&p.record), k, "record")?,
            index: need(p.event, k, "event")?,
            person: need(id(&p.person), k, "person")?,
        },
        "attach_media" => Edit::AttachMedia {
            record: need(id(&p.record), k, "record")?,
            event: p.event,
            media: id(&p.media),
            file: p.file.clone().map(|f| f.trim().to_string()).filter(|f| !f.is_empty()),
        },
        "detach_media" => Edit::DetachMedia {
            record: need(id(&p.record), k, "record")?,
            event: p.event,
            media: need(id(&p.media), k, "media")?,
        },
        other => return Err(format!("unknown edit `{other}`")),
    })
}

fn relation(r: &str) -> Result<gedcom::relations::Relation, String> {
    use gedcom::relations::Relation;
    Ok(match r.trim().to_lowercase().as_str() {
        "father" => Relation::Father,
        "mother" => Relation::Mother,
        "spouse" => Relation::Spouse,
        "child" => Relation::Child,
        "sibling" => Relation::Sibling,
        other => return Err(format!("unknown relation `{other}`: father, mother, spouse, child or sibling")),
    })
}

/// The agent's parameters, turned into a change to check.
pub fn to_request(p: &ChangeParams) -> Result<ChangeRequest, String> {
    let action = match p.action.trim() {
        "fix" => Action::Fix,
        "hide" => Action::Hide,
        "edit" => Action::Edit(p.edits.iter().flatten().map(edit).collect::<Result<_, _>>()?),
        "merge" => Action::Merge(
            p.choices
                .iter()
                .flatten()
                .map(|(fact, who)| (fact.trim().to_uppercase(), who.trim().trim_matches('@').to_string()))
                .collect(),
        ),
        other => return Err(format!("unknown action `{other}`: use fix, edit, merge or hide")),
    };
    let evidence = p
        .evidence
        .as_ref()
        .map(|e| Evidence {
            source: id(&e.source),
            page: e.page.clone().unwrap_or_default().trim().to_string(),
            url: e.url.clone().unwrap_or_default().trim().to_string(),
            quote: e.quote.clone().unwrap_or_default().trim().to_string(),
        })
        .unwrap_or_default();
    Ok(ChangeRequest {
        issue_id: p.issue.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        action,
        reason: p.reason.clone().unwrap_or_default().trim().to_string(),
        evidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_become_a_change() {
        let p: ChangeParams = serde_json::from_value(serde_json::json!({
            "issue": "death_before_birth:I1",
            "action": "edit",
            "edits": [{"kind": "set_event", "record": "@I1@", "event": 3, "date": "12 MAR 1869"}],
            "reason": "The act says 1869",
            "evidence": {"source": "S1", "page": "view 14"}
        }))
        .unwrap();
        let r = to_request(&p).unwrap();
        assert_eq!(
            r.action,
            Action::Edit(vec![Edit::SetEvent {
                record: "I1".into(),
                index: 3,
                value: None,
                date: Some("12 MAR 1869".into()),
                place: None,
                address: None,
                cause: None
            }])
        );
        assert_eq!(r.evidence.source.as_deref(), Some("S1"));
        let bad: ChangeParams =
            serde_json::from_value(serde_json::json!({"issue": "x", "action": "edit", "edits": [{"kind": "set_sex"}]}))
                .unwrap();
        assert!(to_request(&bad).unwrap_err().contains("record"));
        let merge: ChangeParams = serde_json::from_value(serde_json::json!({
            "issue": "possible_duplicate:I10", "action": "merge", "choices": {"birt": "@I11@"},
            "reason": "Same witness of two acts a year apart"
        }))
        .unwrap();
        assert_eq!(to_request(&merge).unwrap().action, Action::Merge(vec![("BIRT".into(), "I11".into())]));
    }

    #[test]
    fn a_correction_needs_no_issue() {
        let p: ChangeParams = serde_json::from_value(serde_json::json!({
            "action": "edit",
            "edits": [{"kind": "set_name", "record": "I27", "given": "Johann"}],
            "reason": "The marriage act of 1850 names the groom Johann",
            "evidence": {"url": "https://example.org/register/51", "quote": "Johann Hörmann"}
        }))
        .unwrap();
        let r = to_request(&p).unwrap();
        assert_eq!(r.issue_id, None);
        assert_eq!(
            r.action,
            Action::Edit(vec![Edit::SetName {
                person: "I27".into(),
                index: 0,
                given: Some("Johann".into()),
                surname: None
            }])
        );
    }
}
