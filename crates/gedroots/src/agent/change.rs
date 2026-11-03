//! `propose_change`: a change for a Data Quality issue (fix, edit, hide), turned into
//! [`gedcom::change`] types. GEDRoots checks it against the file before queueing it in To Review.

use gedcom::change::{Action, Edit, Evidence};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChangeParams {
    /// Issue id, as `quality_issues` gives it.
    pub issue: String,
    /// `fix`: the Quality page's own fix (only when the issue has `automatic_fix`). `edit`: the
    /// edits in `edits`. `hide`: the issue is right (e.g. a marriage really celebrated very young),
    /// and the reason says why.
    pub action: String,
    /// For `edit`: what to change, among the issue's `edits`.
    pub edits: Option<Vec<EditParams>>,
    /// Why: what you read, in a sentence the user can check. Required for `edit` and `hide`.
    pub reason: Option<String>,
    /// What supports the change.
    pub evidence: Option<EvidenceParams>,
}

/// One edit. `kind` says which, and which fields it needs:
/// `set_event` (record, event, date and/or place), `delete_event` (record, event),
/// `set_sex` (record, sex), `set_coordinates` (place, latitude, longitude),
/// `set_media_file` (file), `cite_source` (record, event, source, page),
/// `set_value` (value), `delete_structure` (nothing: the structure the issue designates).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditParams {
    pub kind: String,
    /// Person or family id.
    pub record: Option<String>,
    /// Position of the event in its record (`index` in `person_sheet`).
    pub event: Option<usize>,
    /// GEDCOM date (`12 MAR 1869`, `ABT 1850`); empty to remove it.
    pub date: Option<String>,
    /// Place as the tree names it.
    pub place: Option<String>,
    /// `M`, `F`, `X` or `U`.
    pub sex: Option<String>,
    /// Decimal degrees.
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// Path of the media file, relative to the tree's folder.
    pub file: Option<String>,
    /// Existing source id (`search_sources`).
    pub source: Option<String>,
    /// Reference in the source (page, act number, view).
    pub page: Option<String>,
    pub value: Option<String>,
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
    pub issue_id: String,
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
            if p.date.is_none() && p.place.is_none() {
                return Err("`set_event` needs `date` and/or `place`".into());
            }
            Edit::SetEvent {
                record: need(id(&p.record), k, "record")?,
                index: need(p.event, k, "event")?,
                date: p.date.clone(),
                place: p.place.clone(),
            }
        }
        "delete_event" => {
            Edit::DeleteEvent { record: need(id(&p.record), k, "record")?, index: need(p.event, k, "event")? }
        }
        "set_sex" => Edit::SetSex { person: need(id(&p.record), k, "record")?, sex: need(p.sex.clone(), k, "sex")? },
        "set_coordinates" => Edit::SetCoordinates {
            place: need(p.place.clone(), k, "place")?,
            latitude: need(p.latitude, k, "latitude")?,
            longitude: need(p.longitude, k, "longitude")?,
        },
        "set_media_file" => Edit::SetMediaFile { file: need(p.file.clone(), k, "file")? },
        "cite_source" => Edit::CiteSource {
            record: need(id(&p.record), k, "record")?,
            index: need(p.event, k, "event")?,
            source: need(id(&p.source), k, "source")?,
            page: p.page.clone().unwrap_or_default(),
        },
        "set_value" => Edit::SetValue { value: need(p.value.clone(), k, "value")? },
        "delete_structure" => Edit::DeleteStructure,
        other => return Err(format!("unknown edit `{other}`")),
    })
}

/// The agent's parameters, turned into a change to check.
pub fn to_request(p: &ChangeParams) -> Result<ChangeRequest, String> {
    let action = match p.action.trim() {
        "fix" => Action::Fix,
        "hide" => Action::Hide,
        "edit" => Action::Edit(p.edits.iter().flatten().map(edit).collect::<Result<_, _>>()?),
        other => return Err(format!("unknown action `{other}`: use fix, edit or hide")),
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
        issue_id: p.issue.trim().to_string(),
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
                date: Some("12 MAR 1869".into()),
                place: None
            }])
        );
        assert_eq!(r.evidence.source.as_deref(), Some("S1"));
        let bad: ChangeParams =
            serde_json::from_value(serde_json::json!({"issue": "x", "action": "edit", "edits": [{"kind": "set_sex"}]}))
                .unwrap();
        assert!(to_request(&bad).unwrap_err().contains("record"));
    }
}
