import SwiftUI

/// List of individuals, searchable; pull to sync again.
struct PeopleView: View {
    let model: TreeModel
    @State private var query = ""

    private var filtered: [PersonRow] {
        guard !query.isEmpty else { return model.people }
        return model.people.filter { $0.name.localizedStandardContains(query) }
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
                ForEach(filtered) { person in
                    NavigationLink(value: person.id) {
                        PersonLabel(person: person)
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
            .searchable(text: $query)
            .refreshable { await model.syncIfPaired() }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Menu("More", systemImage: "ellipsis.circle") {
                        Button("Sync Now", systemImage: "arrow.triangle.2.circlepath") {
                            Task { await model.syncIfPaired() }
                        }
                        Button("Forget This Computer", systemImage: "link.badge.minus", role: .destructive) { model.unpair() }
                    }
                }
            }
        }
    }
}

struct PersonLabel: View {
    let person: PersonRow
    var body: some View {
        VStack(alignment: .leading) {
            Text(person.name)
            if !person.years.isEmpty {
                Text(person.years).font(.subheadline).foregroundStyle(.secondary)
            }
        }
    }
}

/// Last sync, and the measurements of the proof of concept.
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
