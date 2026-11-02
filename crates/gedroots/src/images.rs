//! Images on screen: decoded at their display size, kept in a bounded cache.
//!
//! GPUI's `img(path)` decodes the file at full resolution and keeps it until the app closes, in
//! memory and as a GPU texture: a 4000 × 3000 record scan weighs 48 MB, twice. Here, the image is
//! reduced to the step just above its size on screen, and beyond [`BUDGET`] the least recently
//! displayed images are released (memory and texture).
//!
//! `GEDROOTS_DEBUG_IMAGES=1` prints the state of the cache to standard error.

use gedcom::Rect;
use gpui_kit::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

/// Decoded pixels kept at most (excluding images displayed in the last rendered frame).
const BUDGET: usize = 64 << 20;
/// Smallest step, in pixels on the long side.
const MIN_SIDE: f32 = 256.;

/// Image `path`, displayed whole in a `w × h` box (logical pixels) according to `fit`. For a crop,
/// the box is the size of the whole image on screen.
pub fn picture(path: PathBuf, w: f32, h: f32, fit: ObjectFit) -> Img {
    region(path, None, w, h, fit)
}

/// Area `rect` (pixels of the oriented image) of image `path`, decoded alone: the element is only
/// this area, it is fitted and rounded like a whole image (a cropped portrait thus stays a disc).
pub fn picture_region(path: PathBuf, rect: Rect, w: f32, h: f32, fit: ObjectFit) -> Img {
    region(path, Some(rect), w, h, fit)
}

/// Image `path` decoded in detail, for a zoomed view: at full resolution, or [`DETAIL_SIDE`] px on
/// the long side for a larger scan. Stretched to the element's size.
pub fn picture_detail(path: PathBuf) -> Img {
    img(ImageSource::Custom(Arc::new(move |window, cx| {
        let side = image_size(&path).map_or(0, |(w, h)| if w.max(h) > DETAIL_SIDE { DETAIL_SIDE } else { 0 });
        load_side(&path, None, side, window, cx)
    })))
    .object_fit(ObjectFit::Fill)
}

/// Long side of a zoomed image, at most: 64 MB of pixels, within the GPU's texture limits.
const DETAIL_SIDE: u32 = 4096;

fn region(path: PathBuf, rect: Option<Rect>, w: f32, h: f32, fit: ObjectFit) -> Img {
    let contain = matches!(fit, ObjectFit::Contain | ObjectFit::ScaleDown);
    img(ImageSource::Custom(Arc::new(move |window, cx| load(&path, rect, (w, h), contain, window, cx)))).object_fit(fit)
}

thread_local! {
    static SIZES: RefCell<HashMap<PathBuf, Option<(u32, u32)>>> = RefCell::new(HashMap::new());
}

/// Dimensions of the image as displayed (EXIF orientation applied), read from the file header and
/// cached.
pub fn image_size(path: &Path) -> Option<(u32, u32)> {
    if let Some(s) = SIZES.with(|c| c.borrow().get(path).copied()) {
        return s;
    }
    let read = || -> Option<(u32, u32)> {
        use image::ImageDecoder;
        let mut dec = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.into_decoder().ok()?;
        let (w, h) = dec.dimensions();
        let turned = matches!(
            dec.orientation().unwrap_or(image::metadata::Orientation::NoTransforms),
            image::metadata::Orientation::Rotate90
                | image::metadata::Orientation::Rotate270
                | image::metadata::Orientation::Rotate90FlipH
                | image::metadata::Orientation::Rotate270FlipH
        );
        Some(if turned { (h, w) } else { (w, h) })
    };
    let s = read();
    SIZES.with(|c| c.borrow_mut().insert(path.to_path_buf(), s));
    s
}

/// Long side to decode for a `natural` image displayed in `bx` (whole if `contain`, otherwise
/// filling it), at screen scale `scale`; `0`: full resolution. Steps of √2 from 256 px: at worst
/// twice too many pixels, and few variants of the same image.
fn side_for(natural: (u32, u32), bx: (f32, f32), contain: bool, scale: f32) -> u32 {
    let (nw, nh) = (natural.0.max(1) as f32, natural.1.max(1) as f32);
    let (fx, fy) = (bx.0 / nw, bx.1 / nh);
    let f = if contain { fx.min(fy) } else { fx.max(fy) };
    let need = nw.max(nh) * f * scale;
    let mut side = MIN_SIDE;
    while side < need {
        side *= std::f32::consts::SQRT_2;
    }
    let side = side.round() as u32;
    if side >= natural.0.max(natural.1) { 0 } else { side }
}

type Key = (PathBuf, Option<Rect>, u32);

fn load(
    path: &Path,
    rect: Option<Rect>,
    bx: (f32, f32),
    contain: bool,
    window: &mut Window,
    cx: &mut App,
) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
    let natural = match rect {
        Some((_, _, w, h)) => Some((w, h)),
        None => image_size(path),
    };
    let side = natural.map_or(0, |n| side_for(n, bx, contain, window.scale_factor()));
    load_side(path, rect, side, window, cx)
}

/// Image `path` (or its area `rect`) decoded at `side` px on the long side; `0`: full resolution.
fn load_side(
    path: &Path,
    rect: Option<Rect>,
    side: u32,
    window: &mut Window,
    cx: &mut App,
) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
    let key = (path.to_path_buf(), rect, side);
    let res = window.use_asset::<Scaled>(&key, cx)?;
    if let Ok(image) = &res {
        let cache = cx.default_global::<Cache>();
        let frame = cache.frame;
        let entry = cache.entries.entry(key).or_insert_with(|| {
            let bytes = (0..image.frame_count()).map(|i| image.as_bytes(i).map_or(0, |b| b.len())).sum();
            cache.total += bytes;
            Entry { image: image.clone(), bytes, last: frame }
        });
        entry.last = frame;
        if !cache.pending {
            cache.pending = true;
            window.on_next_frame(end_frame);
        }
    }
    Some(res)
}

/// Images decoded by [`Scaled`], with the last rendered frame they appeared in.
#[derive(Default)]
struct Cache {
    entries: HashMap<Key, Entry>,
    total: usize,
    frame: u64,
    pending: bool,
}

impl Global for Cache {}

struct Entry {
    image: Arc<RenderImage>,
    bytes: usize,
    last: u64,
}

/// After rendering: beyond the budget, releases the images displayed longest ago. Those of the
/// frame just rendered are never released.
fn end_frame(window: &mut Window, cx: &mut App) {
    let cache = cx.default_global::<Cache>();
    let mut evicted = Vec::new();
    if cache.total > BUDGET {
        let mut old: Vec<(u64, Key)> =
            cache.entries.iter().filter(|(_, e)| e.last < cache.frame).map(|(k, e)| (e.last, k.clone())).collect();
        old.sort_by_key(|(last, _)| *last);
        for (_, key) in old {
            if cache.total <= BUDGET {
                break;
            }
            let e = cache.entries.remove(&key).unwrap();
            cache.total -= e.bytes;
            evicted.push((key, e.image));
        }
    }
    cache.frame += 1;
    cache.pending = false;
    if std::env::var_os("GEDROOTS_DEBUG_IMAGES").is_some() && !evicted.is_empty() {
        eprintln!("images: {} released, {} cached, {} MB", evicted.len(), cache.entries.len(), cache.total >> 20);
    }
    for (key, image) in evicted {
        cx.remove_asset::<Scaled>(&key);
        let _ = window.drop_image(image);
    }
}

/// Background loading: decoding, then reduction to the requested step.
enum Scaled {}

impl Asset for Scaled {
    type Source = Key;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;

    // No `async fn`: the future must not borrow `App`.
    #[allow(clippy::manual_async_fn)]
    fn load(source: Self::Source, _: &mut App) -> impl Future<Output = Self::Output> + Send + 'static {
        async move {
            let image = decode(&source.0, source.1, source.2)?;
            if std::env::var_os("GEDROOTS_DEBUG_IMAGES").is_some() {
                let s = image.size(0);
                eprintln!("images: {} decoded at {}×{}", source.0.display(), s.width.0, s.height.0);
            }
            Ok(Arc::new(image))
        }
    }
}

/// Simultaneous decodes at most: each goes through the whole image (48 MB for a 4000 × 3000 scan)
/// before reduction, and macOS holds on to released memory for a while. One at a time, the peak
/// stays that of a single image.
const PARALLEL: usize = 1;
static DECODING: (Mutex<usize>, Condvar) = (Mutex::new(0), Condvar::new());

fn decode(path: &Path, rect: Option<Rect>, side: u32) -> Result<RenderImage, ImageCacheError> {
    let (lock, freed) = &DECODING;
    *freed.wait_while(lock.lock().unwrap(), |n| *n >= PARALLEL).unwrap() += 1;
    let res = decode_now(path, rect, side);
    *lock.lock().unwrap() -= 1;
    freed.notify_one();
    res
}

/// Decodes like GPUI (EXIF orientation, BGRA), keeps only `rect` if given, then reduces to `side`
/// px on the long side.
fn decode_now(path: &Path, rect: Option<Rect>, side: u32) -> Result<RenderImage, ImageCacheError> {
    use image::ImageDecoder;
    let mut decoder = image::ImageReader::open(path)?.with_guessed_format()?.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = image::DynamicImage::from_decoder(decoder)?;
    if let Some((x, y, w, h)) = rect {
        // The rectangle is given on the oriented image.
        image.apply_orientation(orientation);
        image = image.crop_imm(x, y, w.max(1), h.max(1));
        if side > 0 && image.width().max(image.height()) > side {
            image = image.thumbnail(side, side);
        }
    } else {
        if side > 0 && image.width().max(image.height()) > side {
            image = image.thumbnail(side, side);
        }
        image.apply_orientation(orientation);
    }
    let mut data = image.into_rgba8();
    for pixel in data.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(RenderImage::new(vec![image::Frame::new(data)]))
}

#[cfg(test)]
mod tests {
    use super::{decode, side_for};

    #[test]
    fn steps() {
        // 96 px thumbnail on a Retina screen: 192 px needed, first step.
        assert_eq!(side_for((4000, 3000), (96., 96.), false, 2.), 256);
        // Media page: 1600 logical px × 2 exceeds the real size.
        assert_eq!(side_for((4000, 3000), (1600., 1200.), false, 2.), 0);
        // Contain: the strongest constraint is the one that counts.
        assert_eq!(side_for((4000, 1000), (400., 400.), true, 1.), 512);
        assert_eq!(side_for((4000, 1000), (400., 400.), false, 1.), 2048);
        // Small image (map tile): always whole.
        assert_eq!(side_for((256, 256), (256., 256.), false, 2.), 0);
    }

    #[test]
    fn downscaling_and_orientation() {
        let dir = std::env::temp_dir().join(format!("gedroots-images-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("large.png");
        image::RgbaImage::from_pixel(1200, 600, image::Rgba([10, 20, 30, 255])).save(&path).unwrap();
        let small = decode(&path, None, 300).unwrap();
        assert_eq!((small.size(0).width.0, small.size(0).height.0), (300, 150));
        // BGRA, comme GPUI.
        assert_eq!(&small.as_bytes(0).unwrap()[..4], &[30, 20, 10, 255]);
        let full = decode(&path, None, 0).unwrap();
        assert_eq!((full.size(0).width.0, full.size(0).height.0), (1200, 600));
        // Area alone (face of a cropped portrait), then reduced.
        let face = decode(&path, Some((100, 50, 400, 200)), 0).unwrap();
        assert_eq!((face.size(0).width.0, face.size(0).height.0), (400, 200));
        let face = decode(&path, Some((100, 50, 400, 200)), 256).unwrap();
        assert_eq!((face.size(0).width.0, face.size(0).height.0), (256, 128));
        std::fs::remove_dir_all(&dir).ok();
    }
}
