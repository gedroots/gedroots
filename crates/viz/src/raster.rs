//! A drawing as pixels, for the app: rendered with resvg (the renderer Typst uses for the books)
//! and the fonts of the app, so that it looks the same on every computer.

use std::sync::{Arc, LazyLock};

/// Inter, the font of the drawings (and of the app).
const FONTS: [&[u8]; 4] = [
    include_bytes!("../../gedroots/assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../../gedroots/assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../../gedroots/assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../../gedroots/assets/fonts/Inter-Bold.ttf"),
];

static OPTIONS: LazyLock<resvg::usvg::Options<'static>> = LazyLock::new(|| {
    let mut db = resvg::usvg::fontdb::Database::new();
    for f in FONTS {
        db.load_font_data(f.to_vec());
    }
    resvg::usvg::Options { font_family: "Inter".into(), fontdb: Arc::new(db), ..Default::default() }
});

/// The drawing, `width` pixels wide, as straight (not premultiplied) BGRA pixels on a
/// transparent background: (width, height, pixels).
pub fn bgra(svg: &str, width: u32) -> Option<(u32, u32, Vec<u8>)> {
    let tree = resvg::usvg::Tree::from_str(svg, &OPTIONS).ok()?;
    let size = tree.size();
    let scale = width as f32 / size.width().max(1.);
    let height = (size.height() * scale).round().max(1.) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width.max(1), height)?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for px in pixmap.pixels() {
        let c = px.demultiply();
        data.extend_from_slice(&[c.blue(), c.green(), c.red(), c.alpha()]);
    }
    Some((width, height, data))
}
