import PDFKit
import SwiftUI

/// The Books tab: the books made in GEDRoots on the computer, typeset here from the synced tree
/// with the same engine (so a book always matches the tree, offline), read page by page and
/// shared as PDF.
struct BooksView: View {
    let model: TreeModel
    @State private var path = NavigationPath()

    var body: some View {
        NavigationStack(path: $path) {
            List {
                if model.tree == nil {
                    ReceivingView(model: model)
                }
                ForEach(model.books, id: \.id) { book in
                    NavigationLink(value: BookLink(id: book.id)) { BookLabel(model: model, book: book) }
                }
            }
            .overlay {
                if model.tree != nil, model.books.isEmpty {
                    ContentUnavailableView("No Books", systemImage: "books.vertical",
                                           description: Text("Books are made in GEDRoots on your computer."))
                }
            }
            .navigationTitle("Books")
            .navigationDestination(for: BookLink.self) { link in
                if let book = model.books.first(where: { $0.id == link.id }) {
                    BookReader(model: model, book: book)
                }
            }
            .refreshable { await model.syncIfPaired() }
            .toolbar { ToolbarItem(placement: .topBarTrailing) { MoreMenu(model: model) } }
        }
        .tint(Theme.accent)
    }
}

struct BookLink: Hashable {
    let id: String
}

extension BookRow {
    /// “Ancestors of Marcel LEMAIRE · 8 generations”.
    var detail: String {
        let n = Int(generations)
        if subject.isEmpty { return String(localized: "\(n) generations") }
        return descendants
            ? String(localized: "Descendants of \(subject) · \(n) generations")
            : String(localized: "Ancestors of \(subject) · \(n) generations")
    }
}

/// A book in the list: its cover once typeset, its title, whose book it is and its pages.
private struct BookLabel: View {
    let model: TreeModel
    let book: BookRow
    @State private var typeset: BookFiles.Typeset?

    var body: some View {
        HStack(spacing: 12) {
            Group {
                if let cover = typeset?.cover {
                    Image(uiImage: cover).resizable().scaledToFit()
                } else {
                    Image(systemName: "book.closed").font(.title2).foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, maxHeight: .infinity).background(Theme.hover)
                }
            }
            .frame(width: 48, height: 68)
            .shadow(color: .black.opacity(0.15), radius: 2, y: 1)
            VStack(alignment: .leading, spacing: 2) {
                Text(book.title).font(.headline)
                Text(book.detail).font(.footnote).foregroundStyle(.secondary)
                if let pages = typeset?.pages {
                    Text("\(pages) pages").font(.footnote).foregroundStyle(.secondary).monospacedDigit()
                }
            }
        }
        .padding(.vertical, 4)
        .task(id: "\(book.id)#\(model.revision)") { typeset = await BookFiles.typeset(book, model: model) }
    }
}

/// A book, page by page: scroll, pinch to zoom, the pages along the bottom; Share sends the PDF
/// (AirDrop, Print, Save to Files or Books).
private struct BookReader: View {
    let model: TreeModel
    let book: BookRow
    @State private var typeset: BookFiles.Typeset?
    @State private var failed = false

    var body: some View {
        Group {
            if let typeset {
                PDFReader(url: typeset.url)
            } else if failed {
                ContentUnavailableView("Can't Typeset This Book", systemImage: "exclamationmark.triangle")
            } else {
                ProgressView("Typesetting…")
            }
        }
        .navigationTitle(book.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.hidden, for: .tabBar)
        .toolbar {
            if let typeset {
                ToolbarItem(placement: .topBarTrailing) {
                    ShareLink(item: typeset.url)
                }
            }
        }
        .task(id: "\(book.id)#\(model.revision)") {
            typeset = await BookFiles.typeset(book, model: model)
            failed = typeset == nil
        }
    }
}

/// PDFKit's view, as in Files: the pages one under the other, the thumbnails along the bottom.
private struct PDFReader: UIViewRepresentable {
    let url: URL

    func makeUIView(context: Context) -> UIView {
        let pdf = PDFView()
        pdf.displayMode = .singlePageContinuous
        pdf.displayDirection = .vertical
        pdf.autoScales = true
        pdf.pageShadowsEnabled = true
        pdf.document = PDFDocument(url: url)
        let thumbnails = PDFThumbnailView()
        thumbnails.pdfView = pdf
        thumbnails.layoutMode = .horizontal
        thumbnails.thumbnailSize = CGSize(width: 28, height: 40)
        thumbnails.backgroundColor = .secondarySystemBackground
        let stack = UIStackView(arrangedSubviews: [pdf, thumbnails])
        stack.axis = .vertical
        thumbnails.heightAnchor.constraint(equalToConstant: 56).isActive = true
        return stack
    }

    func updateUIView(_ view: UIView, context: Context) {
        guard let stack = view as? UIStackView, let pdf = stack.arrangedSubviews.first as? PDFView,
              pdf.document?.documentURL != url
        else { return }
        pdf.document = PDFDocument(url: url)
    }
}

/// The typeset books, as PDF files in the caches: typeset once per book and per version of the
/// tree, named after their title (the name they are shared with).
@MainActor
enum BookFiles {
    struct Typeset {
        let url: URL
        let pages: Int
        let cover: UIImage?
    }

    private static var done: [String: Typeset] = [:]
    private static var running: [String: Task<Typeset?, Never>] = [:]

    private static var root: URL {
        FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Books", isDirectory: true)
    }

    static func typeset(_ book: BookRow, model: TreeModel) async -> Typeset? {
        guard let tree = model.tree, let treeID = model.current?.id else { return nil }
        let key = "\(treeID)/\(book.id)#\(model.revision)"
        if let t = done[key] { return t }
        if let task = running[key] { return await task.value }
        let dir = root.appendingPathComponent(treeID, isDirectory: true)
            .appendingPathComponent(book.id, isDirectory: true)
        let name = fileName(book.title)
        let bookID = book.id
        let task = Task.detached(priority: .userInitiated) { () -> Typeset? in
            let today = Calendar.current.dateComponents([.year, .month, .day], from: Date())
            guard let pdf = try? tree.bookPdf(id: bookID, year: Int32(today.year ?? 2026),
                                              month: UInt8(today.month ?? 1), day: UInt8(today.day ?? 1))
            else { return nil }
            // The previous version of the book goes.
            try? FileManager.default.removeItem(at: dir)
            try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            let url = dir.appendingPathComponent(name).appendingPathExtension("pdf")
            guard (try? Data(pdf).write(to: url)) != nil, let doc = PDFDocument(url: url) else { return nil }
            let cover = doc.page(at: 0)?.thumbnail(of: CGSize(width: 144, height: 204), for: .mediaBox)
            return Typeset(url: url, pages: doc.pageCount, cover: cover)
        }
        running[key] = task
        let result = await task.value
        running[key] = nil
        if let result { done[key] = result }
        return result
    }

    /// Forgets the books of a tree removed from this device.
    static func forget(tree: String) {
        done = done.filter { !$0.key.hasPrefix("\(tree)/") }
        try? FileManager.default.removeItem(at: root.appendingPathComponent(tree, isDirectory: true))
    }

    /// A file name from a book's title: without the characters the systems refuse.
    private static func fileName(_ title: String) -> String {
        let bad = CharacterSet(charactersIn: "/\\:*?\"<>|").union(.controlCharacters)
        let name = title.components(separatedBy: bad).joined(separator: " ").trimmingCharacters(in: .whitespaces)
        return name.isEmpty ? "Book" : name
    }
}
