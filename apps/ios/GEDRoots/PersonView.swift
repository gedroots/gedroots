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
    @Namespace private var zoom

    var body: some View {
        content
            .task(id: "\(id)#\(model.revision)") {
                detail = model.tree?.person(id: id)
                loaded = true
            }
            .toolbar {
                // The tree is another tab here, unlike the desktop.
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Show in Tree", systemImage: "point.3.connected.trianglepath.dotted") {
                        model.showInTree(id)
                    }
                }
            }
            .onChange(of: model.jumps) {
                openEvent = nil
                viewing = nil
            }
    }

    @ViewBuilder private var content: some View {
        if let p = detail {
            List {
                Section { header(p) }
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets(top: 8, leading: 0, bottom: 8, trailing: 0))
                if !p.media.isEmpty {
                    Section { MediaStrip(media: p.media, zoom: zoom, open: { viewing = $0 }) }
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
            .mediaViewer(model: model, media: $viewing, zoom: zoom)
        } else if loaded {
            ContentUnavailableView("Not Found", systemImage: "person.slash")
        }
    }

    private func header(_ p: PersonDetail) -> some View {
        HStack(spacing: 16) {
            Portrait(path: p.portrait, crop: p.portraitCrop, name: p.row.name, size: 72)
            VStack(alignment: .leading, spacing: 2) {
                Text(p.row.name).font(.title3).fontWeight(.semibold)
                let line = [p.row.years, p.occupation ?? ""].filter { !$0.isEmpty }.joined(separator: " · ")
                if !line.isEmpty { Text(line).foregroundStyle(.secondary).monospacedDigit() }
                if let kin = Self.kinshipLine(p) {
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
    static func kinshipLine(_ p: PersonDetail) -> String? {
        var parts: [String] = []
        if let sosa = p.row.sosa { parts.append(String(localized: "Sosa \(sosa)")) }
        if let kin = p.kinship, let root = p.rootName {
            // “ancestor, 6 generations above” → “Ancestor of Marcel Lemaire, 6 generations above”.
            let pieces = kin.split(separator: ",", maxSplits: 1).map { $0.trimmingCharacters(in: .whitespaces) }
            var phrase = String(localized: "\(pieces[0]) of \(root)")
            if pieces.count > 1 { phrase += ", " + pieces[1] }
            parts.append(phrase.prefix(1).uppercased() + phrase.dropFirst())
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    private func links(_ people: [PersonRow]) -> some View {
        ForEach(people) { person in
            NavigationLink(value: PersonLink(id: person.id)) { PersonLabel(person: person) }
                .personMenu(model: model, id: person.id)
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

extension MediaRow: Identifiable {}

extension EventRef: Identifiable {
    public var id: String { "\(owner)#\(index)" }
}
