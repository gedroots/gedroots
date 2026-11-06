//! `propose_change`: typed edits of the tree (a correction read in a document, a fix for a Data
//! Quality issue, a merge, a hidden issue), turned into [`gedcom::change`] types. GEDRoots checks
//! it against the file before queueing it in To Review.

use gedcom::Gedcom;
use gedcom::change::{Action, Edit, Evidence, MergePair};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChangeParams {
    /// Tree to work on, as `file_context` names it in `trees`, when several are open (default: the one in
    /// front in GEDRoots).
    pub tree: Option<String>,
    /// What to change. `apply_fix`, `hide_issue`, `merge_people` and `merge_places` go on their
    /// own; the other edits combine.
    pub edits: Vec<EditParams>,
    /// The Data Quality issue it settles (`quality_issues`), if any: the answer says whether it is
    /// gone. Needed by `apply_fix`, `hide_issue`, `set_value`, `delete_structure` and
    /// `set_media_file`.
    pub issue: Option<String>,
    /// Why: what you read, in a sentence the user can check.
    pub reason: Option<String>,
    /// What supports it; required without `issue` (the source of the tree, the viewer page or a
    /// quote of the document).
    pub evidence: Option<EvidenceParams>,
    /// Id of a waiting proposal of yours (`P3`) that this one corrects and replaces.
    pub replaces: Option<String>,
}

/// One edit. `kind` says which, and which fields it needs:
/// `apply_fix` (the Quality page's own fix of `issue`, when it has `automatic_fix`), `hide_issue`
/// (`issue` is right, `reason` says why; for a possible duplicate, they are two people or two
/// places), `merge_people` (keep, other: two person ids, or those of `issue`; choices),
/// `merge_places` (keep, other: two places as the tree writes them, or those of `issue`; choices),
/// `set_event` (record, event, value, date, place and/or address), `add_event` (record, tag,
/// value, date, place, address), `rename_place` (place, new_place), `delete_event` (record, event),
/// `set_sex` (record, sex), `set_coordinates` (place, latitude, longitude, osm),
/// `set_repository` (record: a source; repository_id or repository, call_number),
/// `set_media_file` (file, relative to the tree's folder), `cite_source` (record, event, source, page, quality),
/// `set_value` (value), `delete_structure` (nothing: the structure the issue designates),
/// `set_name` (record, given and/or surname, name), `add_name` (record, given and/or surname,
/// name_type), `set_age` (record, event, age, person on a family event), `link_relative` and
/// `unlink_relative` (record, relation, person), `set_pedigree` (record, pedigree, person: a
/// parent), `add_note` (record, event or not, text), `set_note` (record, event or not, note, text),
/// `add_associate` (record, event, person, role, phrase), `remove_associate` (record, event,
/// person), `attach_media` (record, event or not; media, or the downloaded file with pdf_page for
/// a PDF), `detach_media` (record, event or not, media).
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
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
    /// For `attach_media`: the image or PDF you downloaded (absolute path); GEDRoots copies it
    /// and files it like the image of an act. For `set_media_file`: the new path, relative to the
    /// tree's folder.
    pub file: Option<String>,
    /// For `attach_media` with a PDF of several pages: the page to keep (from 1).
    pub pdf_page: Option<usize>,
    /// For `merge_people` and `merge_places`: the record kept (person id, or place).
    pub keep: Option<String>,
    /// For `merge_people` and `merge_places`: the record merged into it.
    pub other: Option<String>,
    /// For `merge_people`: for each fact that differs (`merge.conflicts` of the issue, or the
    /// facts the two sheets give differently), the person whose value is right, e.g.
    /// `{"BIRT": "I12"}`. For `merge_places`: for each level that differs (`MAP` for the
    /// coordinates), the place whose value is right. GEDRoots says which are missing.
    pub choices: Option<std::collections::BTreeMap<String, String>>,
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
    /// comments on facts already in the tree; a research lead goes to `note_research`.
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

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
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
    /// Id of the proposal (`P3`), as `resume_research` lists it.
    pub id: String,
    /// Always "waiting for review": the user compares and saves it in GEDRoots.
    pub status: String,
    /// Proposals waiting for review in GEDRoots.
    pub waiting: usize,
    /// With `issue`: is it gone once the change is applied? If not, say why to the user or
    /// complete the change (`replaces`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolves: Option<bool>,
    /// Issues the change would create, to fix as well or to mention to the user.
    pub new_issues: Vec<String>,
}

/// What the app needs to build and check the change.
#[derive(Debug, Clone)]
pub struct ChangeRequest {
    /// `None`: a correction without an issue.
    pub issue_id: Option<String>,
    pub action: Action,
    /// For a merge without an issue: the two records.
    pub pair: Option<MergePair>,
    /// The waiting proposal it replaces.
    pub replaces: Option<String>,
    pub reason: String,
    pub evidence: Evidence,
    /// Its id when it is queued again as the file opens (`P3`); a new one is given otherwise.
    pub id: Option<String>,
    /// The agent that sent it.
    pub agent: String,
    /// The parameters as the agent sent them (JSON), kept in the file while the proposal waits.
    pub request: String,
}

fn id(v: &Option<String>) -> Option<String> {
    v.as_deref().map(|s| s.trim().trim_matches('@').to_string()).filter(|s| !s.is_empty())
}

fn need<T>(v: Option<T>, kind: &str, field: &str) -> Result<T, String> {
    v.ok_or_else(|| format!("`{kind}` needs `{field}`"))
}

fn edit(p: &EditParams, images: bool) -> Result<Edit, String> {
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
            file: match p.file.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
                // A downloaded file goes through the same import as an act's image: copied to the
                // app's cache (a PDF page extracted), shown in To Review, filed when saved.
                Some(f) if Path::new(f).is_absolute() => {
                    let prepared =
                        super::propose::prepare_file(Path::new(f), p.pdf_page, images, &super::propose::cache_dir())?;
                    Some(prepared.file.to_string_lossy().into_owned())
                }
                Some(f) => Some(f.to_string()),
                None => None,
            },
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

/// Edits that go on their own, with the action they become.
const ALONE: &[&str] = &["apply_fix", "hide_issue", "merge_people", "merge_places"];

/// The agent's parameters, turned into a change to check. A downloaded file to attach is prepared
/// here (blocking: a PDF page is rendered), in the format of the file's other records.
pub fn to_request(p: &ChangeParams, ged: &Gedcom) -> Result<ChangeRequest, String> {
    if p.edits.is_empty() {
        return Err("give at least one edit in `edits`".into());
    }
    let alone = p.edits.iter().map(|e| e.kind.trim()).find(|k| ALONE.contains(k));
    if let Some(k) = alone
        && p.edits.len() > 1
    {
        return Err(format!("`{k}` goes on its own: propose the other edits in another `propose_change`"));
    }
    let mut pair = None;
    let action = match (alone, p.edits.first()) {
        (Some("apply_fix"), _) => Action::Fix,
        (Some("hide_issue"), _) => Action::Hide,
        (Some(k), Some(e)) => {
            let places = k == "merge_places";
            let ids = |v: &Option<String>| if places { v.as_deref().map(str::trim).map(str::to_string) } else { id(v) };
            pair = match (ids(&e.keep), ids(&e.other)) {
                (Some(a), Some(b)) if places => Some(MergePair::Places(a, b)),
                (Some(a), Some(b)) => Some(MergePair::People(a, b)),
                (None, None) if p.issue.is_some() => None,
                _ => return Err(format!("`{k}` needs `keep` and `other`, or the `issue` of a possible duplicate")),
            };
            Action::Merge(
                e.choices
                    .iter()
                    .flatten()
                    .map(|(fact, who)| (fact.trim().to_uppercase(), who.trim().trim_matches('@').to_string()))
                    .collect(),
            )
        }
        _ => {
            let images = super::propose::prefers_images(ged);
            Action::Edit(p.edits.iter().map(|e| edit(e, images)).collect::<Result<_, _>>()?)
        }
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
    // Kept as sent, without what only concerns this call.
    let mut request = super::propose::stored_request(p);
    if let Some(o) = request.as_object_mut() {
        o.remove("replaces");
    }
    Ok(ChangeRequest {
        issue_id: p.issue.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        action,
        pair,
        replaces: p.replaces.as_deref().map(str::trim).filter(|r| !r.is_empty()).map(str::to_string),
        reason: p.reason.clone().unwrap_or_default().trim().to_string(),
        evidence,
        id: None,
        agent: String::new(),
        request: request.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(v: serde_json::Value) -> ChangeParams {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn params_become_a_change() {
        let g = Gedcom::default();
        let p = params(serde_json::json!({
            "issue": "death_before_birth:I1",
            "edits": [{"kind": "set_event", "record": "@I1@", "event": 3, "date": "12 MAR 1869"}],
            "reason": "The act says 1869",
            "evidence": {"source": "S1", "page": "view 14"}
        }));
        let r = to_request(&p, &g).unwrap();
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
        let bad = params(serde_json::json!({"issue": "x", "edits": [{"kind": "set_sex"}]}));
        assert!(to_request(&bad, &g).unwrap_err().contains("record"));
        let fix = params(serde_json::json!({"issue": "empty_fact:I1", "edits": [{"kind": "apply_fix"}]}));
        assert_eq!(to_request(&fix, &g).unwrap().action, Action::Fix);
        let mixed = params(serde_json::json!({"issue": "x", "edits": [{"kind": "hide_issue"}, {"kind": "set_sex"}]}));
        assert!(to_request(&mixed, &g).unwrap_err().contains("on its own"));
    }

    #[test]
    fn a_merge_names_its_two_records_or_its_issue() {
        let g = Gedcom::default();
        let merge = params(serde_json::json!({
            "issue": "possible_duplicate:I10",
            "edits": [{"kind": "merge_people", "choices": {"birt": "@I11@"}}],
            "reason": "Same witness of two acts a year apart"
        }));
        let r = to_request(&merge, &g).unwrap();
        assert_eq!((r.action, r.pair), (Action::Merge(vec![("BIRT".into(), "I11".into())]), None));
        let free = params(serde_json::json!({
            "edits": [{"kind": "merge_people", "keep": "@I3@", "other": "I8"}],
            "reason": "The baptism names the same parents"
        }));
        assert_eq!(to_request(&free, &g).unwrap().pair, Some(MergePair::People("I3".into(), "I8".into())));
        let places = params(serde_json::json!({
            "edits": [{"kind": "merge_places", "keep": "Agneaux, Manche", "other": "Agneau, Manche"}],
            "reason": "Same village"
        }));
        assert_eq!(
            to_request(&places, &g).unwrap().pair,
            Some(MergePair::Places("Agneaux, Manche".into(), "Agneau, Manche".into()))
        );
        let half = params(serde_json::json!({"edits": [{"kind": "merge_people", "keep": "I3"}]}));
        assert!(to_request(&half, &g).unwrap_err().contains("`keep` and `other`"));
    }

    #[test]
    fn a_correction_needs_no_issue_and_replaces_one() {
        let g = Gedcom::default();
        let p = params(serde_json::json!({
            "edits": [{"kind": "set_name", "record": "I27", "given": "Johann"}],
            "reason": "The marriage act of 1850 names the groom Johann",
            "evidence": {"url": "https://example.org/register/51", "quote": "Johann Hörmann"},
            "replaces": "P4"
        }));
        let r = to_request(&p, &g).unwrap();
        assert_eq!((r.issue_id, r.replaces.as_deref()), (None, Some("P4")));
        assert!(!r.request.contains("P4"), "replaces is not kept: {}", r.request);
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

    #[test]
    fn a_downloaded_file_is_imported_not_linked_where_it_lies() {
        let g = Gedcom::default();
        let dir = std::env::temp_dir().join(format!("gedroots-attach-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("mariage-1878.png");
        image::RgbImage::new(4, 4).save(&file).unwrap();
        let p = params(serde_json::json!({
            "edits": [{"kind": "attach_media", "record": "S1", "file": file.to_string_lossy()}],
            "reason": "The whole page of the act",
            "evidence": {"url": "https://example.org/view/87"}
        }));
        let r = to_request(&p, &g).unwrap();
        let Action::Edit(edits) = &r.action else { panic!("{:?}", r.action) };
        let Edit::AttachMedia { file: Some(f), .. } = &edits[0] else { panic!("{edits:?}") };
        assert!(Path::new(f).starts_with(super::super::propose::cache_dir()), "{f}");
        let missing = params(serde_json::json!({
            "edits": [{"kind": "attach_media", "record": "S1", "file": dir.join("none.jpg").to_string_lossy()}]
        }));
        assert!(to_request(&missing, &g).unwrap_err().contains("not found"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
