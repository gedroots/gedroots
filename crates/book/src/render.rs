//! Typesetting with Typst, embedded: the template, the data and the charts are virtual files of a
//! minimal Typst world; the fonts are bundled (Inter, the app's, and Source Serif 4 for the text),
//! so a book looks the same on every computer and needs no network.

use std::collections::HashMap;
use std::sync::LazyLock;
use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

const TEMPLATE: &str = include_str!("../assets/book.typ");
const COVER: &str = include_str!("../assets/cover.typ");

/// Fonts of the books: Inter (shared with the app's interface) and Source Serif 4.
const FONT_FILES: [&[u8]; 8] = [
    include_bytes!("../../gedroots/assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../../gedroots/assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../../gedroots/assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../../gedroots/assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../fonts/SourceSerif4-Regular.otf"),
    include_bytes!("../fonts/SourceSerif4-It.otf"),
    include_bytes!("../fonts/SourceSerif4-Semibold.otf"),
    include_bytes!("../fonts/SourceSerif4-SemiboldIt.otf"),
];

struct Fonts {
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

static FONTS: LazyLock<Fonts> = LazyLock::new(|| {
    let fonts: Vec<Font> = FONT_FILES.iter().flat_map(|b| Font::iter(Bytes::new(*b))).collect();
    Fonts { book: LazyHash::new(FontBook::from_fonts(&fonts)), fonts }
});

static LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| LazyHash::new(Library::builder().build()));

fn id(path: &str) -> FileId {
    FileId::new(RootedPath::new(VirtualRoot::Project, VirtualPath::new(path).expect("valid path")))
}

struct BookWorld {
    main: Source,
    files: HashMap<FileId, Bytes>,
    today: (i32, u8, u8),
}

impl World for BookWorld {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &FONTS.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            return Ok(self.main.clone());
        }
        Err(FileError::NotFound(id.vpath().get_with_slash().into()))
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.get(&id).cloned().ok_or_else(|| FileError::NotFound(id.vpath().get_with_slash().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        FONTS.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        Datetime::from_ymd(self.today.0, self.today.1, self.today.2)
    }
}

/// A typeset book: its pages, to export as PDF or to preview.
pub struct Typeset {
    document: PagedDocument,
}

fn messages(errors: &[SourceDiagnostic]) -> String {
    errors.iter().map(|e| e.message.to_string()).collect::<Vec<_>>().join("; ")
}

/// Typesets the template with `data` (JSON) and its files (charts, portraits).
pub fn typeset(data: &str, files: Vec<(String, Vec<u8>)>, today: (i32, u8, u8)) -> Result<Typeset, String> {
    compile(("/book.typ", TEMPLATE), ("/data.json", data), files, today)
}

/// Typesets the cover for a printer (back, spine, front on one spread) with `data` (JSON).
pub fn typeset_cover(data: &str, files: Vec<(String, Vec<u8>)>, today: (i32, u8, u8)) -> Result<Typeset, String> {
    compile(("/cover.typ", COVER), ("/cover.json", data), files, today)
}

fn compile(
    template: (&str, &str),
    data: (&str, &str),
    files: Vec<(String, Vec<u8>)>,
    today: (i32, u8, u8),
) -> Result<Typeset, String> {
    let main = Source::new(id(template.0), template.1.to_string());
    let mut map: HashMap<FileId, Bytes> =
        files.into_iter().map(|(p, b)| (id(&format!("/{p}")), Bytes::new(b))).collect();
    map.insert(id(data.0), Bytes::new(data.1.as_bytes().to_vec()));
    let world = BookWorld { main, files: map, today };
    let document = typst::compile::<PagedDocument>(&world).output.map_err(|e| messages(&e))?;
    Ok(Typeset { document })
}

impl Typeset {
    pub fn page_count(&self) -> usize {
        self.document.pages().len()
    }

    /// The book as PDF.
    pub fn pdf(&self) -> Result<Vec<u8>, String> {
        typst_pdf::pdf(&self.document, &typst_pdf::PdfOptions::default()).map_err(|e| messages(&e))
    }

    /// Size of the pages, in points (all the pages of a book have the same).
    pub fn page_size(&self) -> (f64, f64) {
        self.document.pages().first().map(|p| (p.frame.width().to_pt(), p.frame.height().to_pt())).unwrap_or((1., 1.))
    }

    /// A page (from 0) as BGRA pixels, `width` pixels wide: (width, height, pixels).
    pub fn page_bgra(&self, index: usize, width: u32) -> Option<(u32, u32, Vec<u8>)> {
        let pixmap = typst_render::render(self.document.pages().get(index)?, &self.options(index, width)?);
        let (w, h) = (pixmap.width(), pixmap.height());
        let mut data = pixmap.take();
        // Pages are opaque: premultiplied RGBA is plain RGBA. The app's images are BGRA.
        for px in data.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        Some((w, h, data))
    }

    fn options(&self, index: usize, width: u32) -> Option<typst_render::RenderOptions> {
        let pt = self.document.pages().get(index)?.frame.width().to_pt().max(1.);
        Some(typst_render::RenderOptions { pixel_per_pt: (width as f64 / pt).into(), render_bleed: false })
    }

    /// A page (from 0) as PNG, `width` pixels wide.
    pub fn page_png(&self, index: usize, width: u32) -> Option<Vec<u8>> {
        typst_render::render(self.document.pages().get(index)?, &self.options(index, width)?).encode_png().ok()
    }
}
