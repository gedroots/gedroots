import ImageIO
import SwiftUI

/// Read-only record of an individual, like the desktop's: header, media, timeline (with the
/// relatives' events), parents, unions, cited sources and notes. Empty sections are hidden;
/// every person opens their own record.
struct PersonView: View {
    let model: TreeModel
    let id: String
    @AppStorage("relativesEvents") private var relativesEvents = true
    @State private var openEvent: EventRef?
    @State private var viewing: MediaRow?
    @State private var detail: PersonDetail?
    @State private var loaded = false

    var body: some View {
        content.task(id: "\(id)#\(model.revision)") {
            detail = model.tree?.person(id: id)
            loaded = true
        }
    }

    @ViewBuilder private var content: some View {
        if let p = detail {
            List {
                Section { header(p) }
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets(top: 8, leading: 0, bottom: 8, trailing: 0))
                if !p.media.isEmpty {
                    Section { MediaStrip(media: p.media, open: { viewing = $0 }) }
                        .listRowInsets(EdgeInsets())
                        .listRowBackground(Color.clear)
                }
                let timeline = p.timeline.filter { relativesEvents || $0.kind != .relative }
                if !timeline.isEmpty {
                    Section {
                        ForEach(Array(timeline.enumerated()), id: \.offset) { _, row in
                            Button { openEvent = row.event } label: { TimelineLabel(row: row) }
                                .disabled(row.event == nil)
                        }
                    } header: {
                        HStack {
                            Text("Events")
                            Spacer()
                            Toggle("Relatives’ Events", isOn: $relativesEvents)
                                .toggleStyle(.button).buttonStyle(.borderless).font(.footnote).textCase(nil)
                        }
                    }
                }
                if !p.parents.isEmpty {
                    Section("Parents") { links(p.parents) }
                }
                ForEach(Array(p.unions.enumerated()), id: \.offset) { _, u in
                    Section {
                        if let s = u.spouse { links([s]) }
                        links(u.children)
                    } header: {
                        Text(u.married.isEmpty ? String(localized: "Union") : String(localized: "Married \(u.married)"))
                    }
                }
                if !p.sources.isEmpty {
                    Section("Sources") {
                        ForEach(p.sources, id: \.id) { s in SourceLabel(source: s) }
                    }
                }
                if !p.notes.isEmpty {
                    Section("Notes") {
                        ForEach(Array(p.notes.enumerated()), id: \.offset) { _, n in Text(n) }
                    }
                }
            }
            .navigationBarTitleDisplayMode(.inline)
            .sheet(item: $openEvent) { ref in
                EventSheet(model: model, ref: ref)
                    .presentationDetents([.medium, .large])
            }
            .fullScreenCover(item: $viewing) { m in ImageViewer(model: model, media: m) }
        } else if loaded {
            ContentUnavailableView("Not Found", systemImage: "person.slash")
        }
    }

    private func header(_ p: PersonDetail) -> some View {
        HStack(spacing: 16) {
            Portrait(path: p.portrait, name: p.row.name, size: 72)
            VStack(alignment: .leading, spacing: 2) {
                Text(p.row.name).font(.title3).fontWeight(.semibold)
                let line = [p.row.years, p.occupation ?? ""].filter { !$0.isEmpty }.joined(separator: " · ")
                if !line.isEmpty { Text(line).foregroundStyle(.secondary).monospacedDigit() }
                if let kin = kinshipLine(p) {
                    HStack(spacing: 6) {
                        LineageDot(lineage: p.row.lineage)
                        Text(kin)
                    }
                    .font(.footnote).foregroundStyle(.secondary)
                }
            }
        }
    }

    /// “Sosa 2 · Father of Marcel Lemaire”.
    private func kinshipLine(_ p: PersonDetail) -> String? {
        var parts: [String] = []
        if let sosa = p.row.sosa { parts.append(String(localized: "Sosa \(sosa)")) }
        if let kin = p.kinship, let root = p.rootName {
            let phrase = String(localized: "\(kin) of \(root)")
            parts.append(phrase.prefix(1).uppercased() + phrase.dropFirst())
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    private func links(_ people: [PersonRow]) -> some View {
        ForEach(people) { person in
            NavigationLink(value: person.id) { PersonLabel(person: person) }
        }
    }
}

/// A timeline row: year, then the event and its date and place. Relatives' events are muted.
struct TimelineLabel: View {
    let row: TimelineRow
    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text(row.year).font(.footnote).foregroundStyle(Color.secondary).monospacedDigit()
                .frame(width: 40, alignment: .leading)
            VStack(alignment: .leading, spacing: 2) {
                Text(row.label).foregroundStyle(row.kind == .relative ? Color.secondary : Color.primary)
                let detail = [row.date, row.place].filter { !$0.isEmpty }.joined(separator: " · ")
                if !detail.isEmpty {
                    Text(detail).font(.footnote).foregroundStyle(Color.secondary).monospacedDigit()
                }
            }
        }
        // Buttons tint their label: concrete colours keep the row readable in both appearances.
        .foregroundStyle(Color.primary)
    }
}

struct SourceLabel: View {
    let source: SourceRow
    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(source.title)
            if !source.detail.isEmpty { Text(source.detail).font(.footnote).foregroundStyle(.secondary) }
        }
    }
}

/// Round portrait, or the initials on a neutral background.
struct Portrait: View {
    let path: String?
    let name: String
    let size: CGFloat
    var body: some View {
        Group {
            if let path, let image = Thumbnails.image(path) {
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
                        Thumbnail(path: m.path)
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

/// Small image, decoded at its display size (scans are large) and kept in memory.
struct Thumbnail: View {
    let path: String?
    var body: some View {
        if let path, let image = Thumbnails.image(path) {
            Image(uiImage: image).resizable().scaledToFill()
        } else {
            Theme.hover
        }
    }
}

enum Thumbnails {
    private static let cache = NSCache<NSString, UIImage>()

    static func image(_ path: String, side: CGFloat = 320) -> UIImage? {
        if let hit = cache.object(forKey: path as NSString) { return hit }
        let options = [kCGImageSourceShouldCache: false] as CFDictionary
        guard let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, options),
              let cg = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                  kCGImageSourceCreateThumbnailFromImageAlways: true,
                  kCGImageSourceCreateThumbnailWithTransform: true,
                  kCGImageSourceThumbnailMaxPixelSize: side,
              ] as CFDictionary)
        else { return nil }
        let image = UIImage(cgImage: cg)
        cache.setObject(image, forKey: path as NSString)
        return image
    }
}

extension MediaRow: Identifiable {}

extension EventRef: Identifiable {
    public var id: String { "\(owner)#\(index)" }
}
