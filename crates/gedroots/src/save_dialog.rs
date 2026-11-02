//! Save dialog with a choice of file format, native on each platform:
//! - macOS: an `NSSavePanel` sheet with a Format pop-up in its accessory view, like Preview's
//!   File › Export; choosing a format changes the file name's extension right away;
//! - Windows: the common Save dialog (`IFileSaveDialog`) and its "Save as type" list;
//! - Linux: the desktop portal's file chooser (GNOME, KDE…) with a Format drop-down.
//!
//! GPUI's `prompt_for_new_path` offers no format choice, hence this module. Where the system does
//! not say which format was chosen (a portal without choices), it is inferred from the extension.

use std::path::{Path, PathBuf};

/// A format offered in the dialog.
#[derive(Clone, Debug)]
pub struct Format {
    /// Label shown in the list ("GEDCOM 5.5.1").
    pub label: String,
    /// Extension, without the dot ("ged").
    pub extension: &'static str,
    /// Added after the name when suggesting one, so that two formats with the same extension do
    /// not suggest the same file (" (5.5.1)").
    pub suffix: &'static str,
}

/// What the dialog asks.
pub struct Request {
    /// Window title (Linux), button and title of the sheet elsewhere follow the system.
    pub title: String,
    /// Label of the format list (macOS: "Format:").
    pub format_label: String,
    pub directory: PathBuf,
    /// Suggested name, without suffix nor extension.
    pub stem: String,
    pub formats: Vec<Format>,
    /// Index of the format selected when the dialog opens.
    pub selected: usize,
}

/// The file chosen and the index of its format.
#[derive(Debug, PartialEq)]
pub struct Choice {
    pub path: PathBuf,
    pub format: usize,
}

pub type Answer = Result<Option<Choice>, String>;

/// Opens the dialog attached to `window` and returns its answer, `None` when the user cancels.
/// The future runs on the main thread (`spawn_in`).
pub fn prompt(window: &gpui_kit::Window, request: Request) -> impl Future<Output = Answer> + 'static {
    imp::prompt(window, request)
}

/// Name suggested for `format`: the stem, its suffix and its extension.
pub fn file_name(stem: &str, format: &Format) -> String {
    format!("{stem}{}.{}", format.suffix, format.extension)
}

/// The name typed so far, moved to another format: the extension and suffix of any known format
/// are replaced by those of `to`; the rest of the name is kept.
pub fn rename(name: &str, formats: &[Format], to: usize) -> String {
    let mut stem = name;
    if let Some((base, ext)) = stem.rsplit_once('.')
        && formats.iter().any(|f| f.extension.eq_ignore_ascii_case(ext))
    {
        stem = base;
    }
    if let Some(base) = formats.iter().filter(|f| !f.suffix.is_empty()).find_map(|f| stem.strip_suffix(f.suffix)) {
        stem = base;
    }
    formats.get(to).map(|f| file_name(stem, f)).unwrap_or_else(|| name.to_string())
}

/// `path` with the extension of `format`: kept if it already has it, replaced if it has the
/// extension of another format, added otherwise ("arbre" → "arbre.ged").
pub fn with_extension(path: PathBuf, formats: &[Format], format: usize) -> PathBuf {
    let Some(f) = formats.get(format) else { return path };
    match path.extension().and_then(|e| e.to_str()) {
        Some(e) if e.eq_ignore_ascii_case(f.extension) => path,
        Some(e) if formats.iter().any(|o| o.extension.eq_ignore_ascii_case(e)) => path.with_extension(f.extension),
        _ => {
            let mut s = path.into_os_string();
            s.push(".");
            s.push(f.extension);
            PathBuf::from(s)
        }
    }
}

/// Path to write once the dialog closed on `path` with `format`. If the user kept the suggested
/// name, it follows the chosen format (Windows only swaps the extension, Linux nothing):
/// "Guyot.gdz" chosen as GEDCOM 5.5.1 becomes "Guyot (5.5.1).ged". A name the user typed only gets
/// the format's extension.
pub fn finish(path: PathBuf, request: &Request, format: usize) -> PathBuf {
    let formats = &request.formats;
    let selected = request.selected.min(formats.len().saturating_sub(1));
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let suggested = formats.get(selected).map(|f| file_name(&request.stem, f));
    if format != selected && Some(rename(&name, formats, selected)) == suggested {
        return path.with_file_name(rename(&name, formats, format));
    }
    with_extension(path, formats, format)
}

/// Format of `path` when the system did not say: the selected one if the extension matches it,
/// else the first one with this extension, else the selected one.
#[cfg_attr(not(any(target_os = "linux", target_os = "freebsd")), allow(dead_code))]
pub fn infer(path: &Path, formats: &[Format], selected: usize) -> usize {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    let matches = |f: &Format| f.extension.eq_ignore_ascii_case(ext);
    if formats.get(selected).is_some_and(matches) {
        return selected;
    }
    formats.iter().position(matches).unwrap_or(selected)
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use async_channel::Sender;
    use block2::RcBlock;
    use objc2::rc::{Retained, Weak};
    use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSModalResponse, NSModalResponseOK, NSPopUpButton, NSSavePanel, NSTextField, NSView,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString, NSURL};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    struct Ivars {
        panel: Weak<NSSavePanel>,
        formats: Vec<Format>,
    }

    define_class!(
        // Target of the Format pop-up: renames the file for the chosen format.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "GEDRootsSaveFormatTarget"]
        #[ivars = Ivars]
        struct FormatTarget;

        unsafe impl NSObjectProtocol for FormatTarget {}

        impl FormatTarget {
            #[unsafe(method(formatChanged:))]
            fn format_changed(&self, sender: &NSPopUpButton) {
                let Some(panel) = self.ivars().panel.load() else { return };
                let index = sender.indexOfSelectedItem().max(0) as usize;
                let name = rename(&panel.nameFieldStringValue().to_string(), &self.ivars().formats, index);
                panel.setNameFieldStringValue(&NSString::from_str(&name));
            }
        }
    );

    impl FormatTarget {
        fn new(mtm: MainThreadMarker, ivars: Ivars) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(ivars);
            unsafe { msg_send![super(this), init] }
        }
    }

    pub(super) fn prompt(window: &gpui_kit::Window, request: Request) -> impl Future<Output = Answer> + 'static {
        let (tx, rx) = async_channel::bounded(1);
        open(window, request, tx);
        async move { rx.recv().await.unwrap_or(Ok(None)) }
    }

    fn open(window: &gpui_kit::Window, request: Request, tx: Sender<Answer>) {
        let Some(mtm) = MainThreadMarker::new() else {
            let _ = tx.try_send(Err("the save dialog must be opened on the main thread".into()));
            return;
        };
        // Like GPUI: `savePanel` may return nil despite its annotation.
        let panel: Option<Retained<NSSavePanel>> = unsafe { msg_send![objc2::class!(NSSavePanel), savePanel] };
        let Some(panel) = panel else {
            let _ = tx.try_send(Err("macOS could not create the save dialog".into()));
            return;
        };
        let selected = request.selected.min(request.formats.len().saturating_sub(1));
        panel.setDirectoryURL(Some(
            &NSURL::from_directory_path(&request.directory)
                .unwrap_or_else(|| NSURL::fileURLWithPath(&NSString::from_str(&request.directory.to_string_lossy()))),
        ));
        if let Some(f) = request.formats.get(selected) {
            panel.setNameFieldStringValue(&NSString::from_str(&file_name(&request.stem, f)));
        }
        panel.setCanCreateDirectories(true);
        panel.setExtensionHidden(false);

        // Accessory view: "Format:" and the pop-up, centred.
        let label = NSTextField::labelWithString(&NSString::from_str(&request.format_label), mtm);
        label.sizeToFit();
        let popup = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), NSRect::ZERO, false);
        for f in &request.formats {
            popup.addItemWithTitle(&NSString::from_str(&f.label));
        }
        popup.selectItemAtIndex(selected as isize);
        popup.sizeToFit();
        let target =
            FormatTarget::new(mtm, Ivars { panel: Weak::from_retained(&panel), formats: request.formats.clone() });
        unsafe {
            popup.setTarget(Some(&target as &AnyObject));
            popup.setAction(Some(sel!(formatChanged:)));
        }
        let (l, p) = (label.frame().size, popup.frame().size);
        let (pad, gap) = (12.0, 8.0);
        let height = l.height.max(p.height) + 2.0 * pad;
        let width = l.width + gap + p.width + 2.0 * pad;
        let accessory =
            NSView::initWithFrame(NSView::alloc(mtm), NSRect::new(NSPoint::ZERO, NSSize::new(width, height)));
        let centred = NSAutoresizingMaskOptions::ViewMinXMargin | NSAutoresizingMaskOptions::ViewMaxXMargin;
        label.setFrameOrigin(NSPoint::new(pad, (height - l.height) / 2.0));
        label.setAutoresizingMask(centred);
        popup.setFrameOrigin(NSPoint::new(pad + l.width + gap, (height - p.height) / 2.0));
        popup.setAutoresizingMask(centred);
        accessory.addSubview(&label);
        accessory.addSubview(&popup);
        panel.setAccessoryView(Some(&accessory));

        let handler = RcBlock::new({
            let panel = panel.clone();
            let tx = tx.clone();
            move |response: NSModalResponse| {
                // Keeps the pop-up's target alive as long as the panel (the target is weak).
                let _ = &target;
                let answer = if response == NSModalResponseOK {
                    let index = popup.indexOfSelectedItem().max(0) as usize;
                    panel
                        .URL()
                        .and_then(|url| url.to_file_path())
                        .map(|path| Choice { path: finish(path, &request, index), format: index })
                } else {
                    None
                };
                let _ = tx.try_send(Ok(answer));
            }
        });
        let parent = HasWindowHandle::window_handle(window).ok().and_then(|h| match h.as_raw() {
            RawWindowHandle::AppKit(h) => unsafe { h.ns_view.cast::<NSView>().as_ref() }.window(),
            _ => None,
        });
        match parent {
            Some(parent) => panel.beginSheetModalForWindow_completionHandler(&parent, &handler),
            None => panel.beginWithCompletionHandler(&handler),
        }
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use async_channel::Sender;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{E_FAIL, ERROR_CANCELLED, HWND};
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
        CoUninitialize,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FileSaveDialog, IFileSaveDialog, IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    };
    use windows::core::{HRESULT, HSTRING, PCWSTR};

    pub(super) fn prompt(window: &gpui_kit::Window, request: Request) -> impl Future<Output = Answer> + 'static {
        let (tx, rx) = async_channel::bounded(1);
        open(window, request, tx);
        async move { rx.recv().await.unwrap_or(Ok(None)) }
    }

    fn open(window: &gpui_kit::Window, request: Request, tx: Sender<Answer>) {
        // The handle crosses threads as a number: the dialog runs on its own thread, owned by the
        // window, which it disables while open.
        let owner = HasWindowHandle::window_handle(window).ok().and_then(|h| match h.as_raw() {
            RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
            _ => None,
        });
        std::thread::spawn(move || {
            let answer = unsafe {
                let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
                let answer = show(owner.map(|h| HWND(h as _)), &request).map_err(|e| e.to_string());
                if init.is_ok() {
                    CoUninitialize();
                }
                answer
            };
            let _ = tx.try_send(answer);
        });
    }

    unsafe fn show(owner: Option<HWND>, request: &Request) -> windows::core::Result<Option<Choice>> {
        unsafe {
            let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_ALL)?;
            if let Ok(dir) = request.directory.canonicalize() {
                // `canonicalize` gives a `\\?\` path, which the shell does not parse.
                let dir = dir.to_string_lossy().trim_start_matches(r"\\?\").to_string();
                if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(dir), None) {
                    let _ = dialog.SetFolder(&item);
                }
            }
            // "Save as type": "GEDCOM 5.5.1 (*.ged)". The strings must live until `Show` returns.
            let names: Vec<HSTRING> =
                request.formats.iter().map(|f| HSTRING::from(format!("{} (*.{})", f.label, f.extension))).collect();
            let specs: Vec<HSTRING> =
                request.formats.iter().map(|f| HSTRING::from(format!("*.{}", f.extension))).collect();
            let filters: Vec<COMDLG_FILTERSPEC> = names
                .iter()
                .zip(&specs)
                .map(|(n, s)| COMDLG_FILTERSPEC { pszName: PCWSTR(n.as_ptr()), pszSpec: PCWSTR(s.as_ptr()) })
                .collect();
            dialog.SetFileTypes(&filters)?;
            let selected = request.selected.min(request.formats.len().saturating_sub(1));
            // 1-based. With a default extension, the dialog adds and switches the extension of
            // the chosen type itself.
            dialog.SetFileTypeIndex(selected as u32 + 1)?;
            if let Some(f) = request.formats.get(selected) {
                dialog.SetDefaultExtension(&HSTRING::from(f.extension))?;
                dialog.SetFileName(&HSTRING::from(file_name(&request.stem, f)))?;
            }
            if let Err(e) = dialog.Show(owner) {
                if e.code() == HRESULT::from_win32(ERROR_CANCELLED.0) {
                    return Ok(None);
                }
                return Err(e);
            }
            let format = (dialog.GetFileTypeIndex()?.max(1) - 1) as usize;
            let item = dialog.GetResult()?;
            let raw = item.GetDisplayName(SIGDN_FILESYSPATH)?;
            let path = raw.to_string();
            CoTaskMemFree(Some(raw.0 as _));
            let path = PathBuf::from(path.map_err(|e| windows::core::Error::new(E_FAIL, e.to_string()))?);
            Ok(Some(Choice { path: finish(path, request, format), format }))
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod imp {
    use super::*;
    use ashpd::WindowIdentifier;
    use ashpd::desktop::file_chooser::{Choice, SaveFileRequest};
    use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

    pub(super) fn prompt(window: &gpui_kit::Window, request: Request) -> impl Future<Output = Answer> + 'static {
        let handle = HasWindowHandle::window_handle(window).ok().map(|h| h.as_raw());
        let display = HasDisplayHandle::display_handle(window).ok().map(|h| h.as_raw());
        async move {
            let identifier = match handle {
                Some(h) => WindowIdentifier::from_raw_handle(&h, display.as_ref()).await,
                None => None,
            };
            show(identifier, &request).await
        }
    }

    async fn show(identifier: Option<WindowIdentifier>, request: &Request) -> Answer {
        // A "Format" choice rather than filters: the portal returns the chosen value (ashpd does
        // not expose the chosen filter), and GNOME and KDE show it as a drop-down in the dialog.
        let selected = request.selected.min(request.formats.len().saturating_sub(1));
        let label = request.format_label.trim_end_matches([':', ' ', '\u{a0}', '\u{202f}']);
        let choice =
            request.formats.iter().enumerate().fold(Choice::new(CHOICE, label, &selected.to_string()), |c, (i, f)| {
                c.insert(&i.to_string(), &f.label)
            });
        let name = request.formats.get(selected).map(|f| file_name(&request.stem, f));
        let response = SaveFileRequest::default()
            .identifier(identifier)
            .modal(true)
            .title(request.title.as_str())
            .current_name(name.as_deref())
            .choice(choice)
            .current_folder(&request.directory)
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let files = match response.response() {
            Ok(files) => files,
            Err(ashpd::Error::Response(_)) => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        let Some(path) = files
            .uris()
            .first()
            .and_then(|uri| url::Url::parse(uri.as_str()).ok())
            .and_then(|url| url.to_file_path().ok())
        else {
            return Ok(None);
        };
        // A file chooser without choices: the extension tells the format.
        let chosen = files.choices().iter().find(|(id, _)| id == CHOICE).and_then(|(_, v)| v.parse().ok());
        let format =
            chosen.filter(|&i| i < request.formats.len()).unwrap_or_else(|| infer(&path, &request.formats, selected));
        Ok(Some(super::Choice { path: finish(path, request, format), format }))
    }

    const CHOICE: &str = "format";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formats() -> Vec<Format> {
        vec![
            Format { label: "GEDZIP".into(), extension: "gdz", suffix: "" },
            Format { label: "GEDCOM 5.5.1".into(), extension: "ged", suffix: " (5.5.1)" },
            Format { label: "Geneanet".into(), extension: "ged", suffix: " (Geneanet)" },
        ]
    }

    #[test]
    fn renames_for_the_chosen_format() {
        let f = formats();
        assert_eq!(file_name("Guyot", &f[1]), "Guyot (5.5.1).ged");
        assert_eq!(rename("Guyot.gdz", &f, 1), "Guyot (5.5.1).ged");
        assert_eq!(rename("Guyot (5.5.1).ged", &f, 2), "Guyot (Geneanet).ged");
        assert_eq!(rename("Guyot (Geneanet).ged", &f, 0), "Guyot.gdz");
        // A name typed without extension, or with another one, keeps its dots.
        assert_eq!(rename("Guyot v2", &f, 0), "Guyot v2.gdz");
        assert_eq!(rename("arbre.2026", &f, 1), "arbre.2026 (5.5.1).ged");
    }

    #[test]
    fn fixes_the_extension_after_the_dialog() {
        let f = formats();
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(with_extension(p("/a/Guyot.gdz"), &f, 0), p("/a/Guyot.gdz"));
        assert_eq!(with_extension(p("/a/Guyot.GED"), &f, 1), p("/a/Guyot.GED"));
        assert_eq!(with_extension(p("/a/Guyot.ged"), &f, 0), p("/a/Guyot.gdz"));
        assert_eq!(with_extension(p("/a/Guyot"), &f, 1), p("/a/Guyot.ged"));
        assert_eq!(with_extension(p("/a/arbre.2026"), &f, 0), p("/a/arbre.2026.gdz"));
    }

    #[test]
    fn the_suggested_name_follows_the_format() {
        let request = |selected| Request {
            title: String::new(),
            format_label: String::new(),
            directory: PathBuf::from("/a"),
            stem: "Guyot".into(),
            formats: formats(),
            selected,
        };
        let p = |s: &str| PathBuf::from(s);
        // Linux: the name stays as suggested.
        assert_eq!(finish(p("/a/Guyot.gdz"), &request(0), 2), p("/a/Guyot (Geneanet).ged"));
        // Windows: only the extension follows.
        assert_eq!(finish(p("/a/Guyot (5.5.1).gdz"), &request(1), 0), p("/a/Guyot.gdz"));
        // macOS: already renamed.
        assert_eq!(finish(p("/a/Guyot (Geneanet).ged"), &request(0), 2), p("/a/Guyot (Geneanet).ged"));
        // A name typed by the user is kept.
        assert_eq!(finish(p("/a/Famille.gdz"), &request(0), 1), p("/a/Famille.ged"));
        assert_eq!(finish(p("/a/Famille.ged"), &request(1), 1), p("/a/Famille.ged"));
    }

    #[test]
    fn infers_the_format_from_the_extension() {
        let f = formats();
        assert_eq!(infer(Path::new("a.ged"), &f, 2), 2);
        assert_eq!(infer(Path::new("a.ged"), &f, 0), 1);
        assert_eq!(infer(Path::new("a.gdz"), &f, 1), 0);
        assert_eq!(infer(Path::new("a.txt"), &f, 1), 1);
    }
}
