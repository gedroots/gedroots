//! Updates: GEDRoots updates itself from its GitHub releases, with
//! [Velopack](https://velopack.io).
//!
//! - an installed copy (macOS `.app` from the disk image or the installer, Windows installer,
//!   Linux AppImage) checks the releases at launch and then twice a day, downloads a new version
//!   in the background and installs it when the app quits; GEDRoots › Restart to Update (Help ›
//!   on Windows and Linux) installs it right away;
//! - Check for Updates looks right now and says what it found in the top bar;
//! - the Automatic Updates setting turns the background check off; it belongs to the machine and
//!   lives in the platform's configuration folder, never in the `.ged`.
//!
//! A copy built from the source (`mise run app`) is not managed by Velopack: the update commands
//! are then greyed out.
//!
//! Velopack's calls block (network, files): they run on their own thread, and each change of
//! state is sent to the app's loop, which redraws the windows and the menus.

use gpui_kit::*;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use velopack::sources::GithubSource;
use velopack::{UpdateCheck, UpdateInfo, UpdateManager, VelopackApp};

/// Background check interval while the app stays open.
const INTERVAL: Duration = Duration::from_secs(12 * 60 * 60);

/// Velopack's install, update and uninstall hooks. To call first thing in `main`: on these
/// occasions Velopack runs the app with its own arguments and may exit right away; at a normal
/// launch, it installs an update downloaded during the previous session.
pub fn startup() {
    VelopackApp::build().run();
}

/// Where the update stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Nothing to show: no check yet, or a silent background check that found nothing.
    Idle,
    /// Check started from the menu.
    Checking,
    /// Answer to a check started from the menu.
    UpToDate,
    /// New version being downloaded, in percent. Shown only for a check started from the menu.
    Downloading { version: String, percent: i16, manual: bool },
    /// New version downloaded: installed when the app quits, or with Restart to Update.
    Ready { version: String },
    /// Check started from the menu that failed (no network, GitHub unreachable…).
    Failed,
    /// Restart to Update while a window has unsaved changes.
    SaveFirst { version: String },
}

static STATE: Mutex<State> = Mutex::new(State::Idle);
/// Downloaded update, waiting to be installed.
static PENDING: Mutex<Option<UpdateInfo>> = Mutex::new(None);
/// A check is running (only one at a time).
static BUSY: Mutex<bool> = Mutex::new(false);
/// Queue to the app's loop, created at startup.
static BRIDGE: Mutex<Option<async_channel::Sender<()>>> = Mutex::new(None);

pub fn state() -> State {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn set_state(s: State) {
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) = s;
    if let Some(tx) = BRIDGE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = tx.try_send(());
    }
}

/// Velopack's updater for this copy, or `None` if it was not installed by a GEDRoots package.
fn manager() -> Option<UpdateManager> {
    let source = GithubSource::new(env!("CARGO_PKG_REPOSITORY"), None, false);
    UpdateManager::new(source, None, None).ok()
}

/// True if this copy can update itself.
pub fn available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(|| manager().is_some())
}

/// Main windows, to check for unsaved changes before restarting.
#[derive(Default)]
pub struct Windows(pub Vec<WeakEntity<crate::workspace::Workspace>>);

impl Global for Windows {}

/// Connects the state changes to the app's loop and starts the background checks.
pub fn install(cx: &mut App) {
    cx.set_global(Windows::default());
    let (tx, rx) = async_channel::unbounded::<()>();
    *BRIDGE.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
    cx.spawn(async move |cx| {
        while rx.recv().await.is_ok() {
            cx.update(|cx| {
                crate::commands::install_menus(cx);
                cx.refresh_windows();
            });
        }
    })
    .detach();

    if !available() {
        return;
    }
    std::thread::spawn(|| {
        loop {
            if load().automatic {
                check(false);
            }
            std::thread::sleep(INTERVAL);
        }
    });
}

/// Check for Updates (from the menu): restarts on an update already downloaded, otherwise looks
/// for one.
pub fn check_now(cx: &mut App) {
    match state() {
        State::Ready { version } | State::SaveFirst { version } => restart(version, cx),
        _ => {
            std::thread::spawn(|| check(true));
        }
    }
}

/// Looks for a new version and downloads it (on a thread of its own: the calls block).
fn check(manual: bool) {
    let Some(um) = manager() else { return };
    {
        let mut busy = BUSY.lock().unwrap_or_else(|e| e.into_inner());
        if *busy || PENDING.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
            return;
        }
        *busy = true;
    }
    if manual {
        set_state(State::Checking);
    }
    let result = match um.check_for_updates() {
        Ok(UpdateCheck::UpdateAvailable(info)) => download(&um, *info, manual),
        Ok(UpdateCheck::NoUpdateAvailable | UpdateCheck::RemoteIsEmpty) => {
            set_state(if manual { State::UpToDate } else { State::Idle });
            Ok(())
        }
        Err(e) => Err(e),
    };
    if result.is_err() {
        set_state(if manual { State::Failed } else { State::Idle });
    }
    *BUSY.lock().unwrap_or_else(|e| e.into_inner()) = false;
}

fn download(um: &UpdateManager, info: UpdateInfo, manual: bool) -> Result<(), velopack::Error> {
    let version = info.TargetFullRelease.Version.clone();
    set_state(State::Downloading { version: version.clone(), percent: 0, manual });
    let (tx, rx) = std::sync::mpsc::channel::<i16>();
    let shown = version.clone();
    let progress = std::thread::spawn(move || {
        while let Ok(percent) = rx.recv() {
            set_state(State::Downloading { version: shown.clone(), percent, manual });
        }
    });
    let done = um.download_updates(&info, Some(tx));
    let _ = progress.join();
    done?;
    *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(info);
    set_state(State::Ready { version });
    Ok(())
}

/// Restart to Update: installs the downloaded version and relaunches the app, unless a window has
/// unsaved changes.
fn restart(version: String, cx: &mut App) {
    let unsaved = cx.global::<Windows>().0.iter().filter_map(|w| w.upgrade()).any(|w| w.read(cx).has_unsaved_changes());
    if unsaved {
        set_state(State::SaveFirst { version });
        return;
    }
    if apply(true) {
        cx.quit();
    }
}

/// Hands the downloaded update to Velopack's updater, which waits for the app to exit. Returns
/// `false` if there is nothing to install.
fn apply(restart: bool) -> bool {
    let Some(info) = PENDING.lock().unwrap_or_else(|e| e.into_inner()).clone() else { return false };
    let Some(um) = manager() else { return false };
    um.wait_exit_then_apply_updates(&info, !restart, restart, Vec::<String>::new()).is_ok()
}

/// At quit: installs a downloaded update without relaunching the app.
pub fn apply_on_quit() {
    if matches!(state(), State::Ready { .. } | State::SaveFirst { .. }) {
        apply(false);
    }
}

/// Release notes of this version, on GitHub.
pub fn release_notes_url() -> String {
    format!("{}/releases/tag/v{}", env!("CARGO_PKG_REPOSITORY"), env!("CARGO_PKG_VERSION"))
}

// ----------------------------------------------------------------------
// Setting: `<config>/GEDRoots/updates`, `key=value` lines
// ----------------------------------------------------------------------

/// Saved setting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Check and download in the background (on by default, like most apps).
    pub automatic: bool,
}

fn settings_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("GEDRoots").join("updates"))
}

pub fn load() -> Settings {
    parse(&settings_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default())
}

fn parse(text: &str) -> Settings {
    let mut s = Settings { automatic: true };
    for line in text.lines() {
        if let Some(("automatic", v)) = line.split_once('=') {
            s.automatic = v.trim() != "0";
        }
    }
    s
}

fn save(s: &Settings) {
    let Some(path) = settings_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, format!("automatic={}\n", u8::from(s.automatic)));
}

/// Automatic Updates: turns the background check on or off.
pub fn toggle_automatic(cx: &mut App) {
    let mut s = load();
    s.automatic = !s.automatic;
    save(&s);
    if s.automatic {
        std::thread::spawn(|| check(false));
    }
    crate::commands::install_menus(cx);
}

#[cfg(test)]
mod tests {
    use super::{Settings, parse};

    #[test]
    fn settings_round_trip() {
        assert_eq!(parse(""), Settings { automatic: true });
        assert_eq!(parse("automatic=0\n"), Settings { automatic: false });
        assert_eq!(parse("automatic=1\n"), Settings { automatic: true });
    }

    #[test]
    fn release_notes_point_to_this_version() {
        assert!(super::release_notes_url().ends_with(&format!("/releases/tag/v{}", env!("CARGO_PKG_VERSION"))));
    }
}
