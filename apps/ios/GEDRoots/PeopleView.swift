import SwiftUI

/// List of individuals by surname, searchable with the same keywords as the desktop
/// (`born:1850-1900`, `place:saint-lô`…); pull to sync again.
struct PeopleView: View {
    let model: TreeModel
    @State private var query = ""
    @State private var scope = Scope.all
    @State private var path = NavigationPath()

    private var results: [PersonRow] {
        guard let tree = model.tree else { return [] }
        if query.isEmpty && scope == .all { return model.people }
        return tree.search(query: query, scope: scope)
    }

    /// Sections by the first letter of the surname, like Contacts.
    private var sections: [(String, [PersonRow])] {
        var out: [(String, [PersonRow])] = []
        for p in results {
            let letter = p.surname.first.map { String($0).folding(options: .diacriticInsensitive, locale: nil).uppercased() } ?? "#"
            if out.last?.0 == letter { out[out.count - 1].1.append(p) } else { out.append((letter, [p])) }
        }
        return out
    }

    var body: some View {
        NavigationStack(path: $path) {
            List {
                if model.tree == nil {
                    ContentUnavailableView {
                        Label(model.syncing ? "Receiving Your Tree…" : "No Tree Yet", systemImage: "tree")
                    } description: {
                        if let error = model.error { Text(error) }
                    }
                }
                ForEach(sections, id: \.0) { letter, people in
                    Section(letter) {
                        ForEach(people) { person in
                            NavigationLink(value: person.id) { PersonLabel(person: person) }
                                .personMenu(model: model, id: person.id)
                        }
                    }
                    .indexLabel(letter)
                }
                if model.tree != nil {
                    Section { SyncFooter(model: model) }
                }
            }
            .sectionIndex()
            .navigationTitle("People")
            .navigationDestination(for: String.self) { id in
                PersonView(model: model, id: id)
            }
            .overlay {
                if model.tree != nil, results.isEmpty, !query.isEmpty { ContentUnavailableView.search }
            }
            .treeSearch(model: model, zone: .people, prompt: "Search People", query: $query) { scope = .all }
            .searchScopes($scope, activation: .onSearchPresentation) {
                Text("All").tag(Scope.all)
                Text("Ancestors").tag(Scope.ancestors)
            }
            .refreshable { await model.syncIfPaired() }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) { MoreMenu(model: model) }
            }
        }
        .tint(Theme.accent)
        // A record opened from the iPhone's search, also when it launched the app.
        .onChange(of: model.jumps) { openRequested() }
        .onAppear { openRequested() }
    }

    private func openRequested() {
        guard model.tab == .people, let id = model.personToOpen else { return }
        model.personToOpen = nil
        path = NavigationPath([id])
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

/// The letters along the edge of the list, like Contacts: drag along them to jump to a
/// surname (iOS 26 and later).
private extension View {
    @ViewBuilder func indexLabel(_ letter: String) -> some View {
        if #available(iOS 26.0, *) { sectionIndexLabel(Text(verbatim: letter)) } else { self }
    }

    @ViewBuilder func sectionIndex() -> some View {
        if #available(iOS 26.0, *) { listSectionIndexVisibility(.visible) } else { self }
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
