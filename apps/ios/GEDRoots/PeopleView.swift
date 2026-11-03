import SwiftUI

/// List of individuals by surname, searchable with the same keywords as the desktop
/// (`born:1850-1900`, `place:saint-lô`…); pull to sync again.
struct PeopleView: View {
    let model: TreeModel
    @State private var query = ""
    @State private var scope = Scope.all

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
        NavigationStack {
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
                        }
                    }
                }
                if model.tree != nil {
                    Section { SyncFooter(model: model) }
                }
            }
            .navigationTitle("People")
            .navigationDestination(for: String.self) { id in
                PersonView(model: model, id: id)
            }
            .searchable(text: $query, prompt: Text("Name, place:saint-lô, born:1850-1900"))
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
