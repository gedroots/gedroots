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

/// Full-screen image on the dark viewer background (design.md › Large images): pinch to zoom,
/// double tap to switch between the fitted image and twice its size, drag once zoomed; the records
/// it is linked to below.
struct ImageViewer: View {
    let model: TreeModel
    let media: MediaRow
    @Environment(\.dismiss) private var dismiss
    @State private var scale: CGFloat = 1
    @State private var lastScale: CGFloat = 1
    @State private var offset: CGSize = .zero
    @State private var lastOffset: CGSize = .zero
    /// Decoded once: scans are large, and zooming redraws the view on every frame.
    @State private var image: UIImage?
    @State private var linked: [MediaLink] = []
    @State private var loaded = false

    var body: some View {
        NavigationStack {
            ZStack {
                Theme.viewer.ignoresSafeArea()
                if let image {
                    Image(uiImage: image)
                        .resizable().scaledToFit()
                        .scaleEffect(scale)
                        .offset(offset)
                        .gesture(zoom.simultaneously(with: pan))
                        .onTapGesture(count: 2) {
                            withAnimation(.snappy(duration: 0.2)) {
                                scale = scale > 1 ? 1 : 2
                                lastScale = scale
                                offset = .zero
                                lastOffset = .zero
                            }
                        }
                        .accessibilityLabel(media.title)
                } else if loaded {
                    ContentUnavailableView("Image Not Received", systemImage: "photo")
                        .foregroundStyle(.white)
                }
            }
            .safeAreaInset(edge: .bottom) { links }
            .navigationTitle(media.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(Theme.viewer, for: .navigationBar)
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
        }
        .preferredColorScheme(.dark)
        .task {
            linked = model.tree?.mediaLinks(id: media.id) ?? []
            let path = media.path
            image = await Task.detached { path.flatMap(UIImage.init(contentsOfFile:)) }.value
            loaded = true
        }
    }

    private var zoom: some Gesture {
        MagnifyGesture()
            .onChanged { scale = max(1, min(8, lastScale * $0.magnification)) }
            .onEnded { _ in
                lastScale = scale
                if scale == 1 { offset = .zero; lastOffset = .zero }
            }
    }

    private var pan: some Gesture {
        DragGesture()
            .onChanged { v in
                guard scale > 1 else { return }
                offset = CGSize(width: lastOffset.width + v.translation.width, height: lastOffset.height + v.translation.height)
            }
            .onEnded { _ in lastOffset = offset }
    }

    /// Records the image is linked to.
    @ViewBuilder private var links: some View {
        let links = linked
        if !links.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(Array(links.enumerated()), id: \.offset) { _, l in
                        Text(l.title).font(.subheadline)
                            .padding(.horizontal, 14).frame(height: 36)
                            .background(.black.opacity(0.72), in: Capsule())
                            .overlay(Capsule().stroke(Color(UIColor(rgb: 0x3D3D3D))))
                    }
                }
                .padding(.horizontal, 16)
            }
            .padding(.bottom, 8)
        }
    }
}
