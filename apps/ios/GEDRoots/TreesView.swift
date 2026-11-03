import SwiftUI

/// The trees followed, as menu items: the shown one is checked. Used by the Tree tab's title menu
/// and the “⋯” menu.
struct TreePicker: View {
    let model: TreeModel

    var body: some View {
        Section {
            ForEach(model.trees) { tree in
                // A toggle shows the menu's own check mark; its second text is the subtitle.
                Toggle(isOn: Binding(get: { tree.id == model.currentID }, set: { if $0 { model.select(tree.id) } })) {
                    Text(tree.title)
                    if let detail = TreeDetail(tree: tree).text { Text(detail) }
                }
            }
        }
    }
}

/// Add a Tree… and Manage Trees…, after the list of trees.
struct TreeCommands: View {
    let model: TreeModel

    var body: some View {
        Button("Add a Tree…", systemImage: "qrcode.viewfinder") { model.adding = true }
        Button("Manage Trees…", systemImage: "list.bullet") { model.managing = true }
    }
}

/// Number of people and last sync of a tree.
struct TreeDetail {
    let tree: PairedTree

    var text: String? {
        var parts: [String] = []
        if let n = tree.people { parts.append(String(localized: "\(n) people")) }
        if tree.removed {
            parts.append(String(localized: "Removed From Your Computer"))
        } else if let date = tree.lastSync {
            parts.append(String(localized: "Updated \(date.formatted(.relative(presentation: .named)))"))
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }
}

/// Manage Trees…: the trees followed; a swipe removes one from this device.
struct ManageTreesView: View {
    let model: TreeModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ForEach(model.trees) { tree in
                        Button {
                            model.select(tree.id)
                        } label: {
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(tree.title).foregroundStyle(.primary)
                                    if let detail = TreeDetail(tree: tree).text {
                                        Text(detail).font(.subheadline).foregroundStyle(.secondary)
                                    }
                                }
                                Spacer()
                                if tree.id == model.currentID {
                                    Image(systemName: "checkmark").foregroundStyle(.tint).accessibilityLabel("Shown")
                                }
                            }
                        }
                        .swipeActions {
                            Button("Remove", role: .destructive) { model.remove(tree.id) }
                        }
                    }
                }
                Section {
                    Button("Add a Tree…", systemImage: "qrcode.viewfinder") {
                        dismiss()
                        model.adding = true
                    }
                }
            }
            .navigationTitle("Trees")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
            }
        }
    }
}
