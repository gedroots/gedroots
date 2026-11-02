//! Help › Report an Issue…: the context attached to a GitHub issue.
//!
//! The context is JSON and only holds values chosen by the app, never text taken from the file:
//! version and system, the screen shown (as enum names), record counts and the file's format, the
//! last commands (action names), the last errors (message keys) and the place in the code of the
//! last crash. No name, place, date, identifier or file path can end up in it.

use serde_json::{Value, json};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

/// Commands and errors kept for the report.
const KEPT: usize = 15;
/// Beyond this length, GitHub refuses the new issue URL: the report goes through the clipboard.
const MAX_URL: usize = 7_500;

struct Journal {
    started: Instant,
    commands: VecDeque<(Instant, &'static str)>,
    errors: VecDeque<(Instant, &'static str)>,
}

static JOURNAL: Mutex<Option<Journal>> = Mutex::new(None);

fn with_journal<R>(f: impl FnOnce(&mut Journal) -> R) -> R {
    let mut guard = JOURNAL.lock().unwrap_or_else(|e| e.into_inner());
    let journal = guard.get_or_insert_with(|| Journal {
        started: Instant::now(),
        commands: VecDeque::new(),
        errors: VecDeque::new(),
    });
    f(journal)
}

fn push(list: &mut VecDeque<(Instant, &'static str)>, item: &'static str) {
    if list.len() == KEPT {
        list.pop_front();
    }
    list.push_back((Instant::now(), item));
}

/// Remembers a command run from a shortcut, a menu or the palette (its action name, e.g.
/// `gedroots::AddEvent`).
pub fn record_command(name: &'static str) {
    let name = name.rsplit("::").next().unwrap_or(name);
    // Opening the palette or this report says nothing about the problem.
    if matches!(name, "CommandPalette" | "ReportIssue") {
        return;
    }
    with_journal(|j| {
        if j.commands.back().map(|c| c.1) != Some(name) {
            push(&mut j.commands, name);
        }
    });
}

/// Remembers an error shown in the top bar, by its message key (e.g. `ws.save_failed`).
pub fn record_error(key: &'static str) {
    with_journal(|j| push(&mut j.errors, key));
}

// ----------------------------------------------------------------------
// Crashes: `<config>/GEDRoots/last-crash`, read at the next launch
// ----------------------------------------------------------------------

fn crash_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("GEDRoots").join("last-crash"))
}

/// Writes where the app panicked, for the next report. The panic message is left out: it can
/// quote data from the file.
pub fn install_panic_hook() {
    with_journal(|_| ());
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info.location().map(|l| format!("{}:{}", short_path(l.file()), l.line()));
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let text =
            crash_report(env!("CARGO_PKG_VERSION"), location.as_deref(), std::thread::current().name(), &backtrace);
        if let Some(path) = crash_path() {
            let _ = std::fs::create_dir_all(path.parent().unwrap_or(&path));
            let _ = std::fs::write(path, text);
        }
        previous(info);
    }));
}

/// Source path of a panic, without the folders of the machine that built the app.
fn short_path(file: &str) -> String {
    let file = file.replace('\\', "/");
    for marker in ["/registry/src/", "/git/checkouts/"] {
        if let Some(i) = file.find(marker) {
            // `index.crates.io-…/crate-1.0/src/lib.rs` → `crate-1.0/src/lib.rs`
            let rest = &file[i + marker.len()..];
            return rest.split_once('/').map_or(rest, |(_, r)| r).to_string();
        }
    }
    match file.find("crates/") {
        Some(i) => file[i..].to_string(),
        None => file.rsplit('/').next().unwrap_or(&file).to_string(),
    }
}

/// Crash text: version, location, thread and the functions of GEDRoots on the stack.
fn crash_report(version: &str, location: Option<&str>, thread: Option<&str>, backtrace: &str) -> String {
    let mut out = format!(
        "GEDRoots {version} panicked at {} (thread {})",
        location.unwrap_or("unknown location"),
        thread.unwrap_or("unnamed")
    );
    // Frames are numbered lines (`  12: gedroots::workspace::…`); `at …` lines hold paths.
    let frames: Vec<&str> = backtrace
        .lines()
        .filter_map(|l| l.trim().split_once(": ").map(|(_, f)| f))
        .filter(|f| f.starts_with("gedroots::") || f.starts_with("gedcom::") || f.starts_with("<gedroots::"))
        .filter(|f| !f.starts_with("gedroots::report::"))
        .map(without_hash)
        .take(12)
        .collect();
    for f in frames {
        out.push_str("\n  ");
        out.push_str(f);
    }
    out
}

/// `name::h0123456789abcdef` → `name`.
fn without_hash(frame: &str) -> &str {
    match frame.rsplit_once("::h") {
        Some((name, hash)) if hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()) => name,
        _ => frame,
    }
}

/// The last crash, if the app has crashed since the last report.
pub fn last_crash() -> Option<String> {
    std::fs::read_to_string(crash_path()?).ok().filter(|s| !s.trim().is_empty())
}

/// Forgets the last crash once it has been reported.
pub fn clear_crash() {
    if let Some(path) = crash_path() {
        let _ = std::fs::remove_file(path);
    }
}

// ----------------------------------------------------------------------
// The issue
// ----------------------------------------------------------------------

/// Version, system and machine: nothing that identifies the user.
fn app_and_system() -> Value {
    use sysinfo::{MemoryRefreshKind, RefreshKind, System};
    let memory =
        System::new_with_specifics(RefreshKind::new().with_memory(MemoryRefreshKind::new().with_ram())).total_memory();
    json!({
        "app": {
            "version": env!("CARGO_PKG_VERSION"),
            "build": if cfg!(debug_assertions) { "debug" } else { "release" },
        },
        "system": {
            "os": System::long_os_version().unwrap_or_else(|| std::env::consts::OS.to_string()),
            "kernel": System::kernel_version(),
            "arch": std::env::consts::ARCH,
            "cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
            "memory_gb": (memory as f64 / 1e9 * 10.).round() / 10.,
            "locales": sys_locale::get_locales().take(4).collect::<Vec<_>>(),
        },
    })
}

/// Commands and errors as `[name, seconds ago]`.
fn journal() -> Value {
    let ago = |list: &VecDeque<(Instant, &'static str)>| {
        list.iter().map(|(at, name)| json!([name, at.elapsed().as_secs()])).collect::<Vec<_>>()
    };
    with_journal(|j| {
        json!({
            "minutes_open": j.started.elapsed().as_secs() / 60,
            "last_commands": ago(&j.commands),
            "last_errors": ago(&j.errors),
        })
    })
}

/// The context attached to the issue, as JSON so that the triage and scripts can read it.
/// `window` describes what the window shows (see `Workspace::report_window`); its keys are
/// stable: fields are added, never renamed.
pub fn context(window: Value) -> String {
    let mut out = app_and_system();
    let map = out.as_object_mut().expect("object");
    for part in [window, journal()] {
        if let Value::Object(part) = part {
            map.extend(part);
        }
    }
    if let Some(crash) = last_crash() {
        map.insert("last_crash".into(), Value::String(crash.trim().to_string()));
    }
    one_line_per_key(&out)
}

/// JSON with one line per top-level key: short enough for a URL, easy to read and to parse.
fn one_line_per_key(value: &Value) -> String {
    let Value::Object(map) = value else { return value.to_string() };
    let lines: Vec<String> = map.iter().map(|(k, v)| format!("  {}: {v}", Value::String(k.clone()))).collect();
    format!("{{\n{}\n}}", lines.join(",\n"))
}

/// Body of the issue: the user's description, then the context folded.
pub fn body(description: &str, context: Option<&str>) -> String {
    let mut out = description.trim().to_string();
    if let Some(context) = context {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str("<details><summary>Context</summary>\n\n```json\n");
        out.push_str(context);
        out.push_str("\n```\n\n</details>");
    }
    out.push_str("\n\n_Sent from GEDRoots (Help › Report an Issue)._");
    out
}

/// Title of the issue: the first line of the description, shortened.
pub fn title(description: &str) -> String {
    let line = description.trim().lines().next().unwrap_or("").trim();
    if line.chars().count() <= 80 {
        return line.to_string();
    }
    let cut: String = line.chars().take(79).collect();
    format!("{}…", cut.trim_end())
}

/// New issue address with the title and body filled in. `None` when the body does not fit in a
/// URL: the caller then copies it to the clipboard and opens [`new_issue_url`] with the title
/// alone.
pub fn issue_url(title: &str, body: &str) -> Option<String> {
    let url = format!("{}?title={}&body={}", new_issue_url(), encode(title), encode(body));
    (url.len() <= MAX_URL).then_some(url)
}

/// New issue address, with only the title.
pub fn issue_url_title_only(title: &str) -> String {
    format!("{}?title={}", new_issue_url(), encode(title))
}

fn new_issue_url() -> String {
    format!("{}/issues/new", env!("CARGO_PKG_REPOSITORY"))
}

/// Percent-encoding of a query value (RFC 3986 unreserved characters kept).
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_query_values() {
        assert_eq!(encode("a b&c=d/é"), "a%20b%26c%3Dd%2F%C3%A9");
        assert_eq!(encode("Report-1_2.3~"), "Report-1_2.3~");
    }

    #[test]
    fn context_is_json_with_one_line_per_key() {
        record_command("gedroots::ShowTree");
        let text = context(json!({ "screen": { "view": "Individuals" } }));
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["screen"]["view"], "Individuals");
        assert_eq!(parsed["app"]["version"], env!("CARGO_PKG_VERSION"));
        assert!(parsed["last_commands"].as_array().is_some_and(|c| !c.is_empty()));
        assert!(text.lines().any(|l| l.starts_with("  \"screen\": {")));
    }

    #[test]
    fn title_is_the_first_line_shortened() {
        assert_eq!(title("  Tree is empty\nafter import"), "Tree is empty");
        let long = "x".repeat(120);
        let t = title(&long);
        assert_eq!(t.chars().count(), 80);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn body_folds_the_context() {
        let b = body("It fails", Some("{}"));
        assert!(b.starts_with("It fails\n\n<details><summary>Context</summary>\n\n```json\n{}\n```\n\n</details>"));
        assert!(!body("It fails", None).contains("<details>"));
    }

    #[test]
    fn long_bodies_do_not_fit_in_the_url() {
        assert!(issue_url("t", "short").is_some_and(|u| u.contains("/issues/new?title=t&body=short")));
        assert!(issue_url("t", &"é".repeat(3_000)).is_none());
    }

    #[test]
    fn crash_paths_lose_the_build_machine_folders() {
        assert_eq!(short_path("/home/me/dev/gedroots/crates/gedroots/src/main.rs"), "crates/gedroots/src/main.rs");
        assert_eq!(
            short_path("/Users/me/.cargo/registry/src/index.crates.io-1949/gpui-pre-0.3.10/src/app.rs"),
            "gpui-pre-0.3.10/src/app.rs"
        );
        assert_eq!(short_path("C:\\Users\\me\\x\\crates\\gedcom\\src\\lib.rs"), "crates/gedcom/src/lib.rs");
    }

    #[test]
    fn crash_report_keeps_only_our_frames() {
        let bt = "   0: std::backtrace::Backtrace::create\n             at /rustc/abc/library/std/src/backtrace.rs:331:13\n   \
                  1: gedroots::report::install_panic_hook::{{closure}}\n             at ./crates/gedroots/src/report.rs:70:25\n   \
                  2: gedroots::workspace::Workspace::save_file::h0123456789abcdef\n   3: core::ops::function::Fn::call";
        let r = crash_report("1.0", Some("crates/gedroots/src/workspace.rs:12"), Some("main"), bt);
        assert_eq!(
            r,
            "GEDRoots 1.0 panicked at crates/gedroots/src/workspace.rs:12 (thread main)\n  gedroots::workspace::Workspace::save_file"
        );
    }

    #[test]
    fn keeps_the_last_commands_without_repeats() {
        for _ in 0..3 {
            record_command("gedroots::ShowTree");
        }
        for i in 0..20 {
            record_command(if i % 2 == 0 { "gedroots::AddEvent" } else { "gedroots::SaveFile" });
        }
        let commands = with_journal(|j| j.commands.clone());
        assert_eq!(commands.len(), KEPT);
        assert_eq!(commands.back().map(|c| c.1), Some("SaveFile"));
    }
}
