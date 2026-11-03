//! The agent's writing tools: they only add proposals to review.
//!
//! - the parameters of the `propose_act` tool become a [`Proposal`] (the record entry model),
//!   checked on a copy of the file;
//! - the file downloaded by the agent takes the format of the records already attached: the
//!   relevant page of a PDF becomes a JPEG image (rendered in pure Rust, on every platform), stored
//!   in the app's cache until it is saved.

use gedcom::proposal::{NewSource, Proposal, SourceChoice};
use gedcom::{Act, ActRole, Gedcom, MediaChoice, Participant, PersonChoice, PersonField, RecordKind};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SourceParams {
    /// Existing source id (from `person_sheet` or `search_sources`). Leave empty to create a new source.
    pub id: Option<String>,
    /// New source: register or collection it comes from (e.g. "Rennes, births year X"), stored as PUBL.
    pub register: Option<String>,
    /// New source: title. Leave empty for the app's automatic title ("Birth record of …").
    pub title: Option<String>,
    /// New source: existing repository id (from `search_sources`).
    pub repository_id: Option<String>,
    /// New source: call number of the register at the repository (e.g. "10 NUM 35238").
    pub call_number: Option<String>,
}

/// Who a person of the act is in the tree, decided before proposing (`match_people` lists the
/// candidates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Identity {
    /// A person already in the tree: give their `id`.
    Link,
    /// A new person, created when the act is saved. Refused when someone in the tree resembles
    /// them, unless their ids are listed in `not`.
    Create,
    /// Only named in the citation (witness, neighbour…), nobody is created.
    Mention,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PersonParams {
    /// principal (the child, the deceased, each spouse), father, mother, spouse, witness,
    /// godfather, godmother, declarant, officiator, clergy, neighbor, friend, other.
    pub role: String,
    /// For father, mother, spouse: 0 for the first principal person, 1 for the second spouse.
    pub of: Option<usize>,
    /// link (an existing person, with `id`), create (a new person) or mention (named only).
    pub identity: Identity,
    /// With link: the person's id (from `match_people`, `search_people` or `person_sheet`).
    pub id: Option<String>,
    /// With create: ids of the people of the tree checked and found to be someone else (the
    /// candidates `match_people` or a refusal gave).
    #[serde(default)]
    pub not: Vec<String>,
    pub given: Option<String>,
    pub surname: Option<String>,
    /// M or F.
    pub sex: Option<String>,
    /// Age as written in the record.
    pub age: Option<String>,
    pub occupation: Option<String>,
    pub residence: Option<String>,
    pub birthplace: Option<String>,
    /// Relationship as written ("son", "neighbour"…).
    pub kinship: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProposeParams {
    /// GEDCOM event tag: BIRT, BAPM, CHR, MARR, DEAT, BURI, CENS… Any other event (military
    /// service, conscription, emigration…) is EVEN with `event_type`.
    pub event: String,
    /// Type of the event (GEDCOM TYPE), in the user's words: required with EVEN ("Military
    /// service", "Conscription"…), optional detail for the other events.
    pub event_type: Option<String>,
    /// With EVEN: the event belongs to the couple (first two principals) instead of one person.
    #[serde(default)]
    pub family_event: bool,
    /// Date of the event in GEDCOM 7 format ("4 APR 1802", "FRENCH_R 14 GERM 10", "ABT 1850").
    pub date: Option<String>,
    /// Date the act was written, when it differs from the event (e.g. a burial registered later).
    pub record_date: Option<String>,
    /// Place of the event, as named in the tree when it exists there.
    pub place: Option<String>,
    pub source: SourceParams,
    /// Reference of the act in its source: act number, page, image ("act 27, image 45").
    pub page: Option<String>,
    /// Transcription of the act.
    pub transcription: Option<String>,
    pub people: Vec<PersonParams>,
    /// Absolute path of the downloaded image or PDF of the act (required with `url`).
    pub file: Option<String>,
    /// For a PDF of several pages: page of the act (from 1).
    pub pdf_page: Option<usize>,
    /// Permanent address of the act's page in the archive's viewer; kept as a web link of the source.
    pub url: Option<String>,
}

/// Proposal waiting for review, in the file's window.
#[derive(Debug, Clone)]
pub struct Pending {
    pub proposal: Proposal,
    /// File downloaded by the agent.
    pub original: Option<PathBuf>,
    /// Extracted page of the PDF.
    pub pdf_page: Option<usize>,
    /// Image displayed and attached (the agent's, or the page extracted from the PDF).
    pub image: Option<PathBuf>,
    /// Image shown in the review viewer: `image`, or the rendered page when `image` is a
    /// one-page PDF kept as is.
    pub preview: Option<PathBuf>,
}

/// File to attach to the act, and the image that shows it in the review page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub file: PathBuf,
    pub preview: PathBuf,
}

fn role(s: &str) -> Result<ActRole, String> {
    Ok(match s.trim().to_lowercase().as_str() {
        "principal" | "child" | "deceased" | "husband" | "wife" | "groom" | "bride" => ActRole::Principal,
        "father" => ActRole::Father,
        "mother" => ActRole::Mother,
        "spouse" => ActRole::Spouse,
        "witness" => ActRole::Witness,
        "godfather" => ActRole::Godfather,
        "godmother" => ActRole::Godmother,
        "declarant" | "informant" => ActRole::Declarant,
        "officiator" | "officer" => ActRole::Officiator,
        "clergy" | "priest" => ActRole::Clergy,
        "neighbor" | "neighbour" => ActRole::Neighbor,
        "friend" => ActRole::Friend,
        "other" => ActRole::Other,
        r => return Err(format!("unknown role \"{r}\"")),
    })
}

fn clean_id(s: &str) -> String {
    s.trim().trim_matches('@').to_string()
}

/// Ids the agent checked and ruled out, for each person of the act (same order as `people`).
pub fn ruled_out(p: &ProposeParams) -> Vec<Vec<String>> {
    p.people.iter().map(|x| x.not.iter().map(|s| clean_id(s)).collect()).collect()
}

/// Refuses a person to create who resembles someone of the tree that the agent did not rule out,
/// with the candidates, so that it links them or says they are someone else.
pub fn check_identities(proposal: &Proposal, ruled_out: &[Vec<String>], ged: &Gedcom) -> Result<(), String> {
    let mut problems = Vec::new();
    for (i, found) in proposal.duplicates(ged) {
        let not = ruled_out.get(i).map(Vec::as_slice).unwrap_or_default();
        let left: Vec<_> = found.iter().filter(|s| !not.contains(&s.xref)).collect();
        if left.is_empty() {
            continue;
        }
        let who = proposal.act.participants[i].display_name();
        let list = left
            .iter()
            .map(|s| {
                if s.reason.is_empty() {
                    format!("{} {}", s.xref, s.label)
                } else {
                    format!("{} {} ({})", s.xref, s.label, s.reason)
                }
            })
            .collect::<Vec<_>>()
            .join("; ");
        problems.push(format!("people[{i}] {who} may already be in the tree: {list}"));
    }
    if problems.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{}. Check them with person_sheet: link the same person (identity link with their id), or, if it \
         is someone else, keep identity create and list their ids in `not`.",
        problems.join(". ")
    ))
}

fn opt(s: &Option<String>) -> &str {
    s.as_deref().map(str::trim).unwrap_or("")
}

/// Empty act of this event, at this date, without anyone.
fn base_act(event: &str, family_event: bool, date: &Option<String>) -> Result<Act, String> {
    let event = event.trim().to_uppercase();
    if event.is_empty() {
        return Err("give the event tag (BIRT, MARR, DEAT…)".into());
    }
    let mut act = Act::new("", &event);
    act.family_even = event == "EVEN" && family_event;
    act.date = opt(date).to_string();
    act.participants = Vec::new();
    Ok(act)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MatchPersonParams {
    /// Role in the act, as in `propose_act`.
    pub role: String,
    /// For father, mother, spouse: 0 for the first principal person, 1 for the second spouse.
    pub of: Option<usize>,
    /// Already identified: the person's id. Their relatives in the tree then rank first for the
    /// others (the father of a linked child…).
    pub id: Option<String>,
    pub given: Option<String>,
    pub surname: Option<String>,
    /// M or F.
    pub sex: Option<String>,
    /// Age as written in the record.
    pub age: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MatchParams {
    /// GEDCOM event tag of the act (BIRT, BAPM, MARR, DEAT…, EVEN).
    pub event: String,
    /// With EVEN: the event belongs to the couple.
    #[serde(default)]
    pub family_event: bool,
    /// Date of the act, in GEDCOM 7 format: it rules out people born too late or dead too early.
    pub date: Option<String>,
    /// Everyone named in the act, in the same order as for `propose_act`.
    pub people: Vec<MatchPersonParams>,
}

/// The act described for `match_people`.
pub fn match_act(p: &MatchParams) -> Result<Act, String> {
    let mut act = base_act(&p.event, p.family_event, &p.date)?;
    for person in &p.people {
        let mut part = Participant::new(role(&person.role)?).of(person.of.unwrap_or(0));
        for (f, v) in [
            (PersonField::Given, &person.given),
            (PersonField::Surname, &person.surname),
            (PersonField::Sex, &person.sex),
            (PersonField::Age, &person.age),
        ] {
            if !opt(v).is_empty() {
                part.set(f, opt(v));
            }
        }
        if let Some(x) = person.id.as_deref().map(clean_id).filter(|x| !x.is_empty()) {
            part.choice = PersonChoice::Link(x);
        }
        act.participants.push(part);
    }
    Ok(act)
}

/// Record and source described by the agent (without the file).
pub fn to_proposal(p: &ProposeParams) -> Result<Proposal, String> {
    let mut act = base_act(&p.event, p.family_event, &p.date)?;
    act.event_type = opt(&p.event_type).to_string();
    act.record_date = opt(&p.record_date).to_string();
    act.place = opt(&p.place).to_string();
    act.page = opt(&p.page).to_string();
    act.transcription = opt(&p.transcription).to_string();
    act.participants = Vec::new();
    for person in &p.people {
        let mut part = Participant::new(role(&person.role)?).of(person.of.unwrap_or(0));
        for (f, v) in [
            (PersonField::Given, &person.given),
            (PersonField::Surname, &person.surname),
            (PersonField::Sex, &person.sex),
            (PersonField::Age, &person.age),
            (PersonField::Occupation, &person.occupation),
            (PersonField::Residence, &person.residence),
            (PersonField::Birthplace, &person.birthplace),
            (PersonField::Kinship, &person.kinship),
        ] {
            if !opt(v).is_empty() {
                part.set(f, opt(v));
            }
        }
        let id = person.id.as_deref().map(clean_id).filter(|x| !x.is_empty());
        let who = || format!("people[{}] ({})", act.participants.len(), person.role.trim());
        part.choice = match (person.identity, id) {
            (Identity::Link, Some(x)) => PersonChoice::Link(x),
            (Identity::Link, None) => return Err(format!("{}: identity link needs the person's id", who())),
            (_, Some(_)) => {
                return Err(format!("{}: an id is only given with identity link (the person is in the tree)", who()));
            }
            (Identity::Create, None) => PersonChoice::Create,
            (Identity::Mention, None) => PersonChoice::Mention,
        };
        act.participants.push(part);
    }
    let s = &p.source;
    let source = match &s.id {
        Some(x) if !x.trim().is_empty() => SourceChoice::Existing(x.trim().trim_matches('@').to_string()),
        _ => SourceChoice::New(NewSource {
            title: opt(&s.title).to_string(),
            register: opt(&s.register).to_string(),
            repository: s
                .repository_id
                .as_ref()
                .map(|r| r.trim().trim_matches('@').to_string())
                .filter(|r| !r.is_empty()),
            call_number: opt(&s.call_number).to_string(),
        }),
    };
    Ok(Proposal { source, act, url: opt(&p.url).to_string() })
}

/// Are the records already attached to the file's sources images rather than PDFs?
pub fn prefers_images(ged: &Gedcom) -> bool {
    let (mut images, mut pdfs) = (0usize, 0usize);
    for s in ged.records_of(RecordKind::Source) {
        for o in s.children.iter().filter(|c| c.tag == "OBJE") {
            let Some(m) = o.pointer().and_then(|x| ged.get(x)) else { continue };
            for f in m.children_with("FILE") {
                let mime = gedcom::mime_for(f.val());
                if mime == "application/pdf" {
                    pdfs += 1;
                } else if mime.starts_with("image/") {
                    images += 1;
                }
            }
        }
    }
    images >= pdfs
}

/// File of the record ready to attach: an image, or a single-page PDF if the file keeps its records
/// as PDFs.
pub fn prepare_file(path: &Path, page: Option<usize>, images: bool, cache: &Path) -> Result<Prepared, String> {
    if !path.is_file() {
        return Err(format!("file not found: {}", path.display()));
    }
    let mime = gedcom::mime_for(&path.to_string_lossy());
    if mime.starts_with("image/") {
        let file = copy_image(path, cache)?;
        return Ok(Prepared { preview: file.clone(), file });
    }
    if mime != "application/pdf" {
        return Err("the act's file must be an image or a PDF".into());
    }
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let pdf = hayro::hayro_syntax::Pdf::new(data).map_err(|_| "unreadable PDF (encrypted or damaged)".to_string())?;
    let count = pdf.pages().len();
    let page = match page {
        Some(p) if (1..=count).contains(&p) => p,
        Some(p) => return Err(format!("pdf_page {p} out of range: the PDF has {count} pages")),
        None if count == 1 => 1,
        None => return Err(format!("the PDF has {count} pages: give pdf_page, the page of the act")),
    };
    let jpeg = render_page(&pdf, page - 1)?;
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "act".into());
    let out = cache.join(format!("{stem} p{page}.jpg"));
    std::fs::write(&out, jpeg).map_err(|e| e.to_string())?;
    // A one-page PDF is kept as is when the file stores PDFs; the viewer still needs an image.
    let file = if count == 1 && !images { path.to_path_buf() } else { out.clone() };
    Ok(Prepared { file, preview: out })
}

/// Image downloaded by the agent: read and decoded here, so an unreadable file (no permission,
/// format the app cannot show) is refused with the reason instead of showing a black viewer;
/// then copied to the app's cache, like an extracted PDF page.
fn copy_image(path: &Path, cache: &Path) -> Result<PathBuf, String> {
    let data = std::fs::read(path).map_err(|e| format!("GEDRoots cannot read {}: {e}", path.display()))?;
    let decoded = image::ImageReader::new(std::io::Cursor::new(&data))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| format!("GEDRoots cannot decode {} ({e}): give a JPEG or PNG image", path.display()))?;
    if decoded.width() == 0 || decoded.height() == 0 {
        return Err(format!("{} is an empty image", path.display()));
    }
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "act.jpg".into());
    let out = cache.join(name);
    if out != path {
        std::fs::write(&out, &data).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Renders a page as JPEG, longest side at most 3,000 pixels (readable, without exceeding the
/// resolution of an archive scan).
fn render_page(pdf: &hayro::hayro_syntax::Pdf, index: usize) -> Result<Vec<u8>, String> {
    use hayro::{PixmapSettings, RenderCache, RenderSettings};
    let page = pdf.pages().get(index).ok_or("missing page")?;
    let (w, h) = page.render_dimensions();
    let scale = (3000.0 / w.max(h)).clamp(1.0, 8.0);
    let cache = RenderCache::new();
    let pixmap = hayro::render(
        page,
        &cache,
        &hayro::hayro_interpret::InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings { x_scale: scale, y_scale: scale, bg_color: hayro::vello_cpu::color::palette::css::WHITE },
    );
    let (pw, ph) = (pixmap.width() as u32, pixmap.height() as u32);
    let rgba = pixmap.take_rgba8(hayro::vello_cpu::peniko::ImageAlphaType::AlphaPremultiplied);
    let rgb: Vec<u8> = rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let img = image::RgbImage::from_raw(pw, ph, rgb).ok_or("page not rendered")?;
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90).encode_image(&img).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Attaches the prepared file to the proposed record.
pub fn attach(p: &mut Proposal, image: Option<&Path>) {
    p.act.media = match image {
        Some(i) => MediaChoice::NewFiles(vec![i.to_string_lossy().into_owned()]),
        None => MediaChoice::None,
    };
}

/// Cache folder of the extracted pages.
pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("GEDRoots").join("agent")
}

#[cfg(test)]
mod tests {
    use super::{ProposeParams, check_identities, prepare_file, ruled_out, to_proposal};
    use gedcom::proposal::SourceChoice;

    #[test]
    fn params_become_an_act() {
        let p: ProposeParams = serde_json::from_value(serde_json::json!({
            "event": "birt", "date": "4 APR 1802", "page": "vue 45",
            "source": {"register": "Rennes, naissances an X", "repository_id": "@R1@"},
            "people": [{"role": "principal", "identity": "link", "id": "I2"},
                       {"role": "mother", "identity": "create", "given": "Marie", "surname": "DURAND"},
                       {"role": "witness", "identity": "mention", "given": "Joseph"}]
        }))
        .unwrap();
        let prop = to_proposal(&p).unwrap();
        assert_eq!(prop.act.event, "BIRT");
        assert_eq!(prop.act.participants.len(), 3);
        assert_eq!(prop.act.participants[0].choice, gedcom::PersonChoice::Link("I2".into()));
        assert_eq!(prop.act.participants[2].choice, gedcom::PersonChoice::Mention);
        assert!(matches!(&prop.source, SourceChoice::New(s) if s.repository.as_deref() == Some("R1")));
        let bad: ProposeParams = serde_json::from_value(
            serde_json::json!({"event": "BIRT", "source": {}, "people": [{"role": "uncle", "identity": "create"}]}),
        )
        .unwrap();
        assert!(to_proposal(&bad).unwrap_err().contains("uncle"));
        // Who each person is must be said, and consistently.
        let none = serde_json::from_value::<ProposeParams>(
            serde_json::json!({"event": "BIRT", "source": {}, "people": [{"role": "principal", "given": "Jean"}]}),
        );
        assert!(none.unwrap_err().to_string().contains("identity"));
        for (identity, id) in [("link", None), ("create", Some("I2")), ("mention", Some("I2"))] {
            let p: ProposeParams = serde_json::from_value(serde_json::json!({"event": "BIRT", "source": {},
                "people": [{"role": "principal", "identity": identity, "id": id, "given": "Jean"}]}))
            .unwrap();
            assert!(to_proposal(&p).is_err(), "{identity} {id:?}");
        }
        let milt: ProposeParams = serde_json::from_value(serde_json::json!({
            "event": "even", "event_type": "Military service", "date": "1871", "record_date": "12 MAR 1872",
            "source": {}, "people": [{"role": "principal", "identity": "link", "id": "I2"}]
        }))
        .unwrap();
        let prop = to_proposal(&milt).unwrap();
        assert_eq!((prop.act.event.as_str(), prop.act.event_type.as_str()), ("EVEN", "Military service"));
        assert_eq!(prop.act.record_date, "12 MAR 1872");
        assert!(!prop.act.family_even);
    }

    #[test]
    fn a_new_person_resembling_the_tree_is_refused_until_ruled_out() {
        let ged = gedcom::Gedcom::parse(
            "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 NAME Jean /BLANC/\n1 SEX M\n1 BIRT\n2 DATE 1800\n\
             1 DEAT\n2 DATE 1870\n1 FAMC @F1@\n0 @I2@ INDI\n1 NAME Pierre /BLANC/\n1 SEX M\n1 DEAT\n2 DATE 1840\n\
             1 FAMS @F1@\n0 @F1@ FAM\n1 HUSB @I2@\n1 CHIL @I1@\n0 TRLR\n",
        )
        .unwrap();
        let params = |not: serde_json::Value| -> ProposeParams {
            serde_json::from_value(serde_json::json!({"event": "BAPM", "date": "2 MAR 1800", "source": {},
                "people": [{"role": "principal", "identity": "link", "id": "I1"},
                           {"role": "father", "identity": "create", "given": "Pierre", "surname": "BLANC", "not": not}]}))
            .unwrap()
        };
        let p = params(serde_json::json!([]));
        let err = check_identities(&to_proposal(&p).unwrap(), &ruled_out(&p), &ged).unwrap_err();
        assert!(err.contains("people[1] Pierre BLANC may already be in the tree: I2"), "{err}");
        // Checked and found to be someone else: accepted.
        let p = params(serde_json::json!(["@I2@"]));
        check_identities(&to_proposal(&p).unwrap(), &ruled_out(&p), &ged).unwrap();
    }

    #[test]
    fn refuses_missing_and_foreign_files() {
        let dir = std::env::temp_dir().join(format!("gedroots-propose-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(prepare_file(&dir.join("absent.jpg"), None, true, &dir).is_err());
        let txt = dir.join("a.txt");
        std::fs::write(&txt, "x").unwrap();
        assert!(prepare_file(&txt, None, true, &dir).unwrap_err().contains("image or a PDF"));
        // Not an image after all: refused with the reason, not shown as a black viewer.
        let jpg = dir.join("a.jpg");
        std::fs::write(&jpg, "x").unwrap();
        assert!(prepare_file(&jpg, None, true, &dir).unwrap_err().contains("cannot decode"));
        // A real image is copied to the app's cache.
        let png = dir.join("scan.png");
        image::RgbImage::from_pixel(4, 3, image::Rgb([200, 190, 170])).save(&png).unwrap();
        let cache = dir.join("cache");
        let copied = prepare_file(&png, None, true, &cache).unwrap();
        assert_eq!(copied.file, cache.join("scan.png"));
        assert_eq!(copied.preview, copied.file);
        assert!(copied.file.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// PDF of `pages` A4 pages, each with a black rectangle.
    fn pdf(pages: usize) -> Vec<u8> {
        let mut objs = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            format!(
                "<< /Type /Pages /Count {pages} /Kids [{}] >>",
                (0..pages).map(|i| format!("{} 0 R", 3 + 2 * i)).collect::<Vec<_>>().join(" ")
            ),
        ];
        for i in 0..pages {
            objs.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents {} 0 R >>", 4 + 2 * i));
            let stream = "0 0 0 rg 100 400 300 200 re f";
            objs.push(format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()));
        }
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).bytes());
        for o in offsets {
            out.extend(format!("{o:010} 00000 n \n").bytes());
        }
        out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).bytes());
        out
    }

    #[test]
    fn extracts_the_act_page_of_a_pdf_as_jpeg() {
        let dir = std::env::temp_dir().join(format!("gedroots-pdf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let register = dir.join("registre.pdf");
        std::fs::write(&register, pdf(3)).unwrap();
        // Register of several pages: the record's page is required.
        assert!(prepare_file(&register, None, true, &dir).unwrap_err().contains("3 pages"));
        assert!(prepare_file(&register, Some(4), true, &dir).is_err());
        let jpeg = prepare_file(&register, Some(2), true, &dir).unwrap().file;
        assert_eq!(jpeg.file_name().unwrap(), "registre p2.jpg");
        let img = image::open(&jpeg).unwrap().to_rgb8();
        assert!(img.width().max(img.height()) >= 2900, "{}x{}", img.width(), img.height());
        // White background, black rectangle in the middle of the page.
        assert!(img.get_pixel(10, 10)[0] > 240);
        let (w, h) = (img.width() as f32, img.height() as f32);
        let inside = img.get_pixel((w * 250. / 595.) as u32, (h * (842. - 500.) / 842.) as u32);
        assert!(inside[0] < 20, "{inside:?}");
        // Single-page PDF, in a file that keeps its records as PDFs: kept as is.
        let single = dir.join("acte.pdf");
        std::fs::write(&single, pdf(1)).unwrap();
        // The viewer shows the rendered page, never the PDF itself.
        let kept = prepare_file(&single, None, false, &dir).unwrap();
        assert_eq!(kept.file, single);
        assert_eq!(kept.preview.file_name().unwrap(), "acte p1.jpg");
        assert!(image::open(&kept.preview).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
