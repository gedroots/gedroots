import SwiftUI

/// The Media tab: a dense grid like Photos, all media or only photos or documents.
struct MediaView: View {
    let model: TreeModel
    @State private var kind = MediaKind.all
    @State private var viewing: MediaRow?
    @State private var items: [MediaRow] = []

    var body: some View {
        NavigationStack {
            ScrollView {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 110), spacing: 2)], spacing: 2) {
                    ForEach(items) { m in
                        Button { viewing = m } label: {
                            Color.clear.aspectRatio(1, contentMode: .fit)
                                .overlay { Thumbnail(path: m.path) }
                                .clipped()
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(m.title)
                    }
                }
            }
            .overlay {
                if items.isEmpty { ContentUnavailableView("No Media", systemImage: "photo.on.rectangle") }
            }
            .safeAreaInset(edge: .top) {
                Picker("Type", selection: $kind) {
                    Text("All").tag(MediaKind.all)
                    Text("Photos").tag(MediaKind.photos)
                    Text("Documents").tag(MediaKind.documents)
                }
                .pickerStyle(.segmented)
                .padding(.horizontal, 16)
                .padding(.bottom, 8)
                .background(.bar)
            }
            .navigationTitle("Media")
            .navigationBarTitleDisplayMode(.inline)
            .refreshable { await model.syncIfPaired() }
            .toolbar { ToolbarItem(placement: .topBarTrailing) { MoreMenu(model: model) } }
            .fullScreenCover(item: $viewing) { m in ImageViewer(model: model, media: m) }
        }
        .tint(Theme.accent)
        .task(id: "\(kind)#\(model.revision)") { items = (model.tree?.media(kind: kind) ?? []).filter(\.isImage) }
    }
}

/// Full-screen image on the dark viewer background (design.md › Large images), zoomed natively:
/// pinch, double tap, pan. The people identified on it are framed with their names, and the
/// records it is linked to are listed below; a tap opens them.
struct ImageViewer: View {
    let model: TreeModel
    let media: MediaRow
    @Environment(\.dismiss) private var dismiss
    /// Decoded once, off the main thread: scans are large.
    @State private var image: UIImage?
    @State private var faces: [FaceBox] = []
    @State private var linked: [MediaLink] = []
    @State private var loaded = false
    @State private var stack = NavigationPath()

    var body: some View {
        NavigationStack(path: $stack) {
            ZStack {
                Theme.viewer.ignoresSafeArea()
                if let image {
                    ZoomableImage(image: image, faces: faces, open: { stack.append($0) })
                        .accessibilityLabel(media.title)
                } else if loaded {
                    ContentUnavailableView("Image Not Received", systemImage: "photo")
                        .foregroundStyle(.white)
                }
            }
            .safeAreaInset(edge: .bottom) { links }
            .environment(\.colorScheme, .dark)
            .navigationTitle(media.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(Theme.viewer, for: .navigationBar)
            .toolbarBackground(.visible, for: .navigationBar)
            .toolbarColorScheme(.dark, for: .navigationBar)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Close", systemImage: "xmark") { dismiss() }
                }
                if let path = media.path {
                    ToolbarItem(placement: .topBarTrailing) {
                        ShareLink(item: URL(fileURLWithPath: path))
                    }
                }
            }
            .navigationDestination(for: String.self) { id in PersonView(model: model, id: id) }
            .navigationDestination(for: SourceLink.self) { link in SourceView(model: model, id: link.id) }
        }
        .tint(Theme.accent)
        .task { await load() }
    }

    private func load() async {
        let links = model.tree?.mediaLinks(id: media.id) ?? []
        linked = links
        guard let path = media.path else {
            loaded = true
            return
        }
        let decoded = await Task.detached { () -> (UIImage, [FaceBox])? in
            guard let source = ImageFile.source(path), let size = ImageFile.size(source),
                  let cg = ImageFile.decode(source, side: 4096)
            else { return nil }
            // The areas are in pixels of the file; the decoded image may be smaller.
            let k = CGFloat(cg.width) / size.width
            let faces = links.compactMap { l -> FaceBox? in
                guard let id = l.person, let area = l.crop?.rect(in: size) else { return nil }
                return FaceBox(id: id, name: l.title, rect: CGRect(x: area.minX * k, y: area.minY * k,
                                                                    width: area.width * k, height: area.height * k))
            }
            return (UIImage(cgImage: cg), faces)
        }.value
        image = decoded?.0
        faces = decoded?.1 ?? []
        loaded = true
    }

    /// Records the image is linked to.
    @ViewBuilder private var links: some View {
        if !linked.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(Array(linked.enumerated()), id: \.offset) { _, l in
                        Button {
                            if let id = l.person {
                                stack.append(id)
                            } else if let id = l.source {
                                stack.append(SourceLink(id: id))
                            }
                        } label: {
                            Text(l.title).font(.subheadline).foregroundStyle(.white)
                                .padding(.horizontal, 14).frame(height: 36)
                                .background(.black.opacity(0.72), in: Capsule())
                                .overlay(Capsule().stroke(Color(UIColor(rgb: 0x3D3D3D))))
                        }
                        .buttonStyle(.plain)
                        .disabled(l.person == nil && l.source == nil)
                    }
                }
                .padding(.horizontal, 16)
            }
            .padding(.bottom, 8)
        }
    }
}
