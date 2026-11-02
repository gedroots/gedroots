//! Images à l'écran : décodées à leur taille d'affichage, gardées dans un cache borné.
//!
//! `img(chemin)` de GPUI décode le fichier en pleine résolution et le garde jusqu'à
//! la fermeture de l'app, en mémoire et en texture GPU : un scan d'acte de
//! 4000 × 3000 pèse 48 Mo, deux fois. Ici, l'image est réduite au palier juste
//! au-dessus de sa taille à l'écran, et au-delà de [`BUDGET`] les images les moins
//! récemment affichées sont libérées (mémoire et texture).
//!
//! `GEDROOTS_DEBUG_IMAGES=1` affiche l'état du cache sur la sortie d'erreur.

use gedcom::Rect;
use gpui_kit::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

/// Pixels décodés gardés au plus (hors images affichées dans la dernière image rendue).
const BUDGET: usize = 64 << 20;
/// Plus petit palier, en pixels sur le grand côté.
const MIN_SIDE: f32 = 256.;

/// L'image `path`, affichée en entier dans une boîte `w × h` (pixels logiques)
/// selon `fit`. Pour un recadrage, la boîte est la taille de l'image entière à l'écran.
pub fn picture(path: PathBuf, w: f32, h: f32, fit: ObjectFit) -> Img {
    region(path, None, w, h, fit)
}

/// La zone `rect` (pixels de l'image orientée) de l'image `path`, décodée seule :
/// l'élément n'est que cette zone, il se cadre et s'arrondit comme une image entière
/// (un portrait recadré reste ainsi un disque).
pub fn picture_region(path: PathBuf, rect: Rect, w: f32, h: f32, fit: ObjectFit) -> Img {
    region(path, Some(rect), w, h, fit)
}

fn region(path: PathBuf, rect: Option<Rect>, w: f32, h: f32, fit: ObjectFit) -> Img {
    let contain = matches!(fit, ObjectFit::Contain | ObjectFit::ScaleDown);
    img(ImageSource::Custom(Arc::new(move |window, cx| load(&path, rect, (w, h), contain, window, cx)))).object_fit(fit)
}

thread_local! {
    static SIZES: RefCell<HashMap<PathBuf, Option<(u32, u32)>>> = RefCell::new(HashMap::new());
}

/// Dimensions de l'image telle qu'affichée (orientation EXIF appliquée),
/// lues dans l'en-tête du fichier et mises en cache.
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

/// Grand côté à décoder pour une image `natural` affichée dans `bx` (en entier si
/// `contain`, sinon en la remplissant), à l'échelle d'écran `scale` ; `0` : pleine résolution. Paliers de √2 à partir de 256 px :
/// au pire deux fois trop de pixels, et peu de variantes d'une même image.
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

/// Images décodées par [`Scaled`], avec la dernière image rendue où elles figuraient.
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

/// Après le rendu : au-delà du budget, libère les images les plus anciennement
/// affichées. Celles de l'image qui vient d'être rendue ne sont jamais libérées.
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

/// Chargement en tâche de fond : décodage puis réduction au palier demandé.
enum Scaled {}

impl Asset for Scaled {
    type Source = Key;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;

    // Pas d'`async fn` : le futur ne doit pas emprunter `App`.
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

/// Décodages simultanés au plus : chacun passe par l'image entière (48 Mo pour un
/// scan de 4000 × 3000) avant réduction, et macOS garde un moment la mémoire libérée.
/// Un à la fois, ce pic reste celui d'une seule image.
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

/// Décode comme GPUI (orientation EXIF, BGRA), ne garde que `rect` s'il est donné,
/// puis réduit à `side` px sur le grand côté.
fn decode_now(path: &Path, rect: Option<Rect>, side: u32) -> Result<RenderImage, ImageCacheError> {
    use image::ImageDecoder;
    let mut decoder = image::ImageReader::open(path)?.with_guessed_format()?.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = image::DynamicImage::from_decoder(decoder)?;
    if let Some((x, y, w, h)) = rect {
        // Le rectangle est donné sur l'image orientée.
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
    fn paliers() {
        // Vignette de 96 px sur écran Retina : 192 px utiles, premier palier.
        assert_eq!(side_for((4000, 3000), (96., 96.), false, 2.), 256);
        // Fiche d'un média : 1600 px logiques × 2 dépasse la taille réelle.
        assert_eq!(side_for((4000, 3000), (1600., 1200.), false, 2.), 0);
        // Contain : c'est la contrainte la plus forte qui compte.
        assert_eq!(side_for((4000, 1000), (400., 400.), true, 1.), 512);
        assert_eq!(side_for((4000, 1000), (400., 400.), false, 1.), 2048);
        // Petite image (tuile de carte) : toujours en entier.
        assert_eq!(side_for((256, 256), (256., 256.), false, 2.), 0);
    }

    #[test]
    fn reduction_et_orientation() {
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
        // Zone seule (visage d'un portrait recadré), puis réduite.
        let face = decode(&path, Some((100, 50, 400, 200)), 0).unwrap();
        assert_eq!((face.size(0).width.0, face.size(0).height.0), (400, 200));
        let face = decode(&path, Some((100, 50, 400, 200)), 256).unwrap();
        assert_eq!((face.size(0).width.0, face.size(0).height.0), (256, 128));
        std::fs::remove_dir_all(&dir).ok();
    }
}
