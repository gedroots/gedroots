import ImageIO
import SwiftUI
import UIKit

/// Round portrait, or the initials on a neutral background. The portrait shows the area of the
/// photo the person was identified in (`CROP`), like the desktop's.
struct Portrait: View {
    let path: String?
    var crop: ImageCrop?
    let name: String
    let size: CGFloat
    var body: some View {
        Group {
            if let path, let image = Thumbnails.image(path, crop: crop) {
                Image(uiImage: image).resizable().scaledToFill()
            } else {
                Text(initials).font(.system(size: size / 3.6, weight: .semibold)).foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity).background(Theme.hover)
            }
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
        .accessibilityHidden(true)
    }

    /// First letters of the first given name and of the surname.
    private var initials: String {
        let words = name.split(separator: " ")
        let letters = words.count > 1 ? [words.first!, words.last!] : words
        return letters.compactMap(\.first).map(String.init).joined().uppercased()
    }
}

/// Horizontal strip of the record's images (the portrait first); a tap opens the viewer.
struct MediaStrip: View {
    let media: [MediaRow]
    var open: ((MediaRow) -> Void)?
    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(media.filter(\.isImage), id: \.id) { m in
                    Button { open?(m) } label: {
                        Thumbnail(path: m.path, crop: m.crop)
                            .frame(width: 96, height: 72)
                            .clipShape(RoundedRectangle(cornerRadius: 8))
                    }
                    .buttonStyle(.plain)
                    .disabled(open == nil)
                    .accessibilityLabel(m.title)
                }
            }
        }
        // Aligned with the list's content and section headers.
        .contentMargins(.horizontal, 16, for: .scrollContent)
    }
}

/// Small image, decoded at its display size (scans are large) and kept in memory; only the
/// record's area of it when the link has one.
struct Thumbnail: View {
    let path: String?
    var crop: ImageCrop?
    var body: some View {
        if let path, let image = Thumbnails.image(path, crop: crop) {
            Image(uiImage: image).resizable().scaledToFill()
        } else {
            Theme.hover
        }
    }
}

extension ImageCrop {
    /// The area on an image of `size` pixels, clamped to it like the desktop does; `nil` when
    /// it is empty.
    func rect(in size: CGSize) -> CGRect? {
        let x = min(CGFloat(left), size.width)
        let y = min(CGFloat(top), size.height)
        let w = min(width.map { CGFloat($0) } ?? .infinity, size.width - x)
        let h = min(height.map { CGFloat($0) } ?? .infinity, size.height - y)
        return w > 0 && h > 0 ? CGRect(x: x, y: y, width: w, height: h) : nil
    }
}

enum ImageFile {
    static func source(_ path: String) -> CGImageSource? {
        CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary)
    }

    /// Size of the image as displayed, EXIF orientation applied: the space `CROP` is written in.
    static func size(_ source: CGImageSource) -> CGSize? {
        guard let p = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let w = (p[kCGImagePropertyPixelWidth] as? NSNumber)?.doubleValue,
              let h = (p[kCGImagePropertyPixelHeight] as? NSNumber)?.doubleValue
        else { return nil }
        // Orientations 5 to 8 turn the image a quarter.
        let turned = ((p[kCGImagePropertyOrientation] as? NSNumber)?.intValue ?? 1) >= 5
        return turned ? CGSize(width: h, height: w) : CGSize(width: w, height: h)
    }

    /// Decoded with its orientation, the longer side at most `side` pixels.
    static func decode(_ source: CGImageSource, side: CGFloat) -> CGImage? {
        CGImageSourceCreateThumbnailAtIndex(source, 0, [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: side,
        ] as CFDictionary)
    }
}

enum Thumbnails {
    private static let cache = NSCache<NSString, UIImage>()

    /// The image, or only its `crop` area, decoded so that it is about `side` pixels.
    static func image(_ path: String, crop: ImageCrop? = nil, side: CGFloat = 320) -> UIImage? {
        let area = crop.map { "\($0.left),\($0.top),\($0.width ?? 0),\($0.height ?? 0)" } ?? ""
        let key = NSString(string: "\(path)#\(area)#\(side)")
        if let hit = cache.object(forKey: key) { return hit }
        guard let source = ImageFile.source(path) else { return nil }
        var cg: CGImage?
        if let crop, let size = ImageFile.size(source), let rect = crop.rect(in: size), rect.size != size {
            // Decoded large enough that the area itself is about `side` pixels.
            let longest = max(size.width, size.height)
            let target = min(longest, (side * longest / max(rect.width, rect.height)).rounded(.up))
            if let full = ImageFile.decode(source, side: target) {
                let k = CGFloat(full.width) / size.width
                cg = full.cropping(to: CGRect(x: rect.minX * k, y: rect.minY * k,
                                              width: rect.width * k, height: rect.height * k).integral)
            }
        } else {
            cg = ImageFile.decode(source, side: side)
        }
        guard let cg else { return nil }
        let image = UIImage(cgImage: cg)
        cache.setObject(image, forKey: key)
        return image
    }
}

/// A person identified on a photo, framed where they are.
struct FaceBox: Equatable {
    let id: String
    let name: String
    /// In pixels of the displayed image.
    let rect: CGRect
}

/// The viewer's image, zoomed by UIKit itself: pinch, pan with momentum and bounce, double tap
/// to zoom on a point and back, the identified people framed with their names.
struct ZoomableImage: UIViewRepresentable {
    let image: UIImage
    let faces: [FaceBox]
    let open: (String) -> Void

    func makeUIView(context: Context) -> ZoomView { ZoomView() }

    func updateUIView(_ view: ZoomView, context: Context) {
        view.open = open
        view.show(image, faces: faces)
    }
}

final class ZoomView: UIView, UIScrollViewDelegate {
    private let scroll = UIScrollView()
    private let imageView = UIImageView()
    /// The frames are above the image, not in it: they keep their thickness at any zoom.
    private let overlay = PassThroughView()
    private var faces: [FaceBox] = []
    private var faceViews: [FaceView] = []
    private var fitted = false
    var open: ((String) -> Void)?

    override init(frame: CGRect) {
        super.init(frame: frame)
        scroll.delegate = self
        scroll.showsVerticalScrollIndicator = false
        scroll.showsHorizontalScrollIndicator = false
        scroll.contentInsetAdjustmentBehavior = .never
        scroll.decelerationRate = .fast
        scroll.bouncesZoom = true
        scroll.addSubview(imageView)
        addSubview(scroll)
        addSubview(overlay)
        let double = UITapGestureRecognizer(target: self, action: #selector(doubleTap(_:)))
        double.numberOfTapsRequired = 2
        scroll.addGestureRecognizer(double)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    func show(_ image: UIImage, faces: [FaceBox]) {
        if imageView.image !== image {
            imageView.image = image
            scroll.zoomScale = 1
            imageView.frame = CGRect(origin: .zero, size: image.size)
            scroll.contentSize = image.size
            fitted = false
            setNeedsLayout()
        }
        if faces != self.faces {
            self.faces = faces
            faceViews.forEach { $0.removeFromSuperview() }
            faceViews = faces.map { face in
                let view = FaceView(face: face)
                view.addAction(UIAction { [weak self] _ in self?.open?(face.id) }, for: .touchUpInside)
                overlay.addSubview(view)
                return view
            }
            placeFaces()
        }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        scroll.frame = bounds
        overlay.frame = bounds
        guard let image = imageView.image, image.size.width > 0, image.size.height > 0, bounds.width > 0 else { return }
        let fit = min(bounds.width / image.size.width, bounds.height / image.size.height)
        // Fitted until the person zooms; a rotation refits a fitted image.
        let atFit = !fitted || abs(scroll.zoomScale - scroll.minimumZoomScale) < 0.0001
        scroll.minimumZoomScale = fit
        scroll.maximumZoomScale = max(fit * 8, 1)
        if atFit { scroll.zoomScale = fit }
        fitted = true
        center()
    }

    func viewForZooming(in scrollView: UIScrollView) -> UIView? { imageView }
    func scrollViewDidZoom(_ scrollView: UIScrollView) { center() }
    func scrollViewDidScroll(_ scrollView: UIScrollView) { placeFaces() }

    /// Keeps an image smaller than the screen in the middle.
    private func center() {
        let size = imageView.frame.size
        scroll.contentInset = UIEdgeInsets(top: max(0, (bounds.height - size.height) / 2),
                                           left: max(0, (bounds.width - size.width) / 2), bottom: 0, right: 0)
        placeFaces()
    }

    private func placeFaces() {
        for (view, face) in zip(faceViews, faces) {
            view.frame = imageView.convert(face.rect, to: overlay)
        }
    }

    @objc private func doubleTap(_ tap: UITapGestureRecognizer) {
        if scroll.zoomScale > scroll.minimumZoomScale * 1.01 {
            scroll.setZoomScale(scroll.minimumZoomScale, animated: true)
        } else {
            let point = tap.location(in: imageView)
            let scale = min(scroll.maximumZoomScale, scroll.minimumZoomScale * 3)
            let size = CGSize(width: bounds.width / scale, height: bounds.height / scale)
            scroll.zoom(to: CGRect(x: point.x - size.width / 2, y: point.y - size.height / 2,
                                   width: size.width, height: size.height), animated: true)
        }
    }
}

/// Lets touches through to the image, except on the frames.
private final class PassThroughView: UIView {
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hit = super.hitTest(point, with: event)
        return hit === self ? nil : hit
    }
}

/// A frame around an identified person, their name under it; a tap opens their record.
private final class FaceView: UIControl {
    private let label = UILabel()

    init(face: FaceBox) {
        super.init(frame: .zero)
        layer.borderColor = UIColor.white.withAlphaComponent(0.9).cgColor
        layer.borderWidth = 2
        layer.cornerRadius = 4
        label.text = face.name
        label.font = .preferredFont(forTextStyle: .caption1)
        label.adjustsFontForContentSizeCategory = true
        label.textColor = .white
        label.textAlignment = .center
        label.backgroundColor = UIColor.black.withAlphaComponent(0.72)
        label.layer.cornerRadius = 4
        label.layer.masksToBounds = true
        addSubview(label)
        isAccessibilityElement = true
        accessibilityLabel = face.name
        accessibilityTraits = .button
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    override var isHighlighted: Bool {
        didSet { alpha = isHighlighted ? 0.6 : 1 }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        let size = label.intrinsicContentSize
        let w = size.width + 12
        label.frame = CGRect(x: (bounds.width - w) / 2, y: bounds.height + 4, width: w, height: size.height + 4)
    }

    /// The name is part of the target.
    override func point(inside point: CGPoint, with event: UIEvent?) -> Bool {
        bounds.contains(point) || label.frame.contains(point)
    }
}
