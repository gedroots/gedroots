import SwiftUI

/// Read-only record of an individual: portrait, events, family.
struct PersonView: View {
    let model: TreeModel
    let id: String

    var body: some View {
        if let p = model.tree?.person(id: id) {
            List {
                if let path = p.portrait, let image = UIImage(contentsOfFile: path) {
                    Section {
                        Image(uiImage: image)
                            .resizable().scaledToFill()
                            .frame(width: 120, height: 120)
                            .clipShape(Circle())
                            .frame(maxWidth: .infinity)
                    }
                    .listRowBackground(Color.clear)
                }
                if !p.events.isEmpty {
                    Section("Events") {
                        ForEach(Array(p.events.enumerated()), id: \.offset) { _, e in
                            VStack(alignment: .leading, spacing: 2) {
                                HStack {
                                    Text(e.label).font(.headline)
                                    Spacer()
                                    Text(e.date).foregroundStyle(.secondary)
                                }
                                if !e.place.isEmpty {
                                    Text(e.place).font(.subheadline).foregroundStyle(.secondary)
                                }
                            }
                        }
                    }
                }
                if !p.parents.isEmpty {
                    Section("Parents") { links(p.parents) }
                }
                ForEach(Array(p.unions.enumerated()), id: \.offset) { _, u in
                    Section(u.spouse?.name ?? String(localized: "Unknown Spouse")) {
                        if let s = u.spouse { links([s]) }
                        links(u.children)
                    }
                }
            }
            .navigationTitle(p.row.name)
            .navigationBarTitleDisplayMode(.inline)
        } else {
            ContentUnavailableView("Not Found", systemImage: "person.slash")
        }
    }

    private func links(_ people: [PersonRow]) -> some View {
        ForEach(people) { person in
            NavigationLink(value: person.id) { PersonLabel(person: person) }
        }
    }
}
