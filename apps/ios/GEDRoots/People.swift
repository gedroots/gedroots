import SwiftUI

/// The people found by the Tree's search, or before anything is typed, the records opened
/// last. Same criteria as the desktop (`born:1850-1900`, `place:saint-lô`…), as tokens.
struct PeopleResults: View {
    let model: TreeModel
    let query: String
    let scope: Scope

    private var browsing: Bool { query.isEmpty && scope == .all }

    private var results: [PersonRow] {
        guard let tree = model.tree else { return [] }
        if browsing {
            let byID = Dictionary(model.people.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            return model.recent.compactMap { byID[$0] }
        }
        return tree.search(query: query, scope: scope)
    }

    var body: some View {
        let rows = results
        List {
            Section {
                ForEach(rows) { person in
                    NavigationLink(value: PersonLink(id: person.id)) { PersonLabel(person: person) }
                        .personMenu(model: model, id: person.id)
                }
            } header: {
                if browsing && !rows.isEmpty { Text("Recent") }
            }
        }
        .overlay {
            if rows.isEmpty && !browsing { ContentUnavailableView.search }
        }
    }
}

/// A person in a list: lineage dot, name, then years and place.
struct PersonLabel: View {
    let person: PersonRow
    var body: some View {
        HStack(spacing: 8) {
            LineageDot(lineage: person.lineage)
            VStack(alignment: .leading, spacing: 2) {
                Text(person.name).fontWeight(person.lineage == .root ? .semibold : .regular)
                let detail = [person.years, person.place].filter { !$0.isEmpty }.joined(separator: " · ")
                if !detail.isEmpty {
                    Text(detail).font(.footnote).foregroundStyle(.secondary).monospacedDigit()
                }
            }
        }
    }
}

/// Last sync.
struct SyncFooter: View {
    let model: TreeModel
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if model.syncing {
                Label("Syncing…", systemImage: "arrow.triangle.2.circlepath")
                if let p = model.progress, p.received < p.total {
                    ProgressView(value: p.fraction)
                }
            } else if let date = model.lastSync {
                Text("Updated \(date, format: .relative(presentation: .named))")
            }
            if let error = model.error, model.tree != nil {
                Text(error).font(.footnote).foregroundStyle(.red)
            }
        }
        .foregroundStyle(.secondary)
    }
}

extension View {
    /// Long press on a person: their header as a preview, and Show in Tree.
    func personMenu(model: TreeModel, id: String) -> some View {
        contextMenu {
            Button("Show in Tree", systemImage: "point.3.connected.trianglepath.dotted") { model.showInTree(id) }
        } preview: {
            PersonPreview(model: model, id: id)
        }
    }
}

/// A person's header, for previews. The portrait is decoded at once: a preview is drawn once.
struct PersonPreview: View {
    let model: TreeModel
    let id: String
    var body: some View {
        if let p = model.tree?.person(id: id) {
            HStack(spacing: 16) {
                Group {
                    if let path = p.portrait, let image = Thumbnails.image(path, crop: p.portraitCrop) {
                        Image(uiImage: image).resizable().scaledToFill()
                    } else {
                        Image(systemName: "person.fill").font(.title).foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, maxHeight: .infinity).background(Theme.hover)
                    }
                }
                .frame(width: 64, height: 64)
                .clipShape(Circle())
                VStack(alignment: .leading, spacing: 2) {
                    Text(p.row.name).font(.headline)
                    let line = [p.row.years, p.occupation ?? ""].filter { !$0.isEmpty }.joined(separator: " · ")
                    if !line.isEmpty { Text(line).foregroundStyle(.secondary).monospacedDigit() }
                    if !p.row.place.isEmpty { Text(p.row.place).font(.footnote).foregroundStyle(.secondary) }
                    if let kin = PersonView.kinshipLine(p) {
                        Text(kin).font(.footnote).foregroundStyle(.secondary)
                    }
                }
            }
            .padding(16)
            .frame(width: 320, alignment: .leading)
        }
    }
}
