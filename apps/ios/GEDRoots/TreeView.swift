import SwiftUI

/// The Tree tab: the grandparents, the parents, the person (the root person at first), then their
/// unions and children. A tap on a relative centres the tree on them; a tap on the person opens
/// their record. Lineages are told by a dot and by their place in the tree, never colour alone.
struct TreeView: View {
    let model: TreeModel
    @State private var focus: String?
    @State private var path = NavigationPath()

    private var current: String? { focus ?? model.tree?.root() }

    var body: some View {
        NavigationStack(path: $path) {
            Group {
                if let id = current, let f = model.tree?.family(id: id) {
                    ScrollView {
                        FamilyChart(family: f, center: center, open: { path.append($0) })
                            .padding(16)
                    }
                    .refreshable { await model.syncIfPaired() }
                } else {
                    ContentUnavailableView {
                        Label(model.syncing ? "Receiving Your Tree…" : "No Tree Yet", systemImage: "tree")
                    } description: {
                        if let error = model.error { Text(error) }
                    }
                }
            }
            .navigationTitle("Tree")
            .navigationBarTitleDisplayMode(.inline)
            .navigationDestination(for: String.self) { id in PersonView(model: model, id: id) }
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Root Person", systemImage: "house") { withAnimation { focus = nil } }
                        .disabled(focus == nil)
                }
                ToolbarItem(placement: .topBarTrailing) { MoreMenu(model: model) }
            }
        }
        .tint(Theme.accent)
    }

    private func center(_ id: String) {
        withAnimation(.snappy(duration: 0.2)) { focus = id }
    }
}

/// Generations as rows, the oldest at the top.
struct FamilyChart: View {
    let family: FamilyView
    let center: (String) -> Void
    let open: (String) -> Void

    var body: some View {
        VStack(spacing: 24) {
            HStack(spacing: 8) {
                ForEach(0..<4, id: \.self) { i in box(family.grandparents[i], compact: true) }
            }
            HStack(spacing: 8) {
                ForEach(0..<2, id: \.self) { i in box(family.parents[i], compact: false) }
            }
            Button { open(family.person.id) } label: {
                HStack(spacing: 12) {
                    Portrait(path: family.portrait, name: family.person.name, size: 56)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(family.person.name).font(.subheadline).fontWeight(.semibold)
                        detail(family.person)
                    }
                    Spacer(minLength: 0)
                }
                .padding(12)
                .frame(maxWidth: 280)
                .background(Theme.accentSoft, in: RoundedRectangle(cornerRadius: 8))
                .overlay(RoundedRectangle(cornerRadius: 8).stroke(Theme.accent, lineWidth: 2))
            }
            .buttonStyle(.plain)
            .accessibilityHint(Text("Opens the record"))
            ForEach(Array(family.unions.enumerated()), id: \.offset) { _, u in
                VStack(spacing: 12) {
                    if let s = u.spouse {
                        VStack(spacing: 4) {
                            if !u.married.isEmpty {
                                Text("⚭ \(u.married)").font(.footnote).foregroundStyle(.secondary).monospacedDigit()
                            }
                            box(s, compact: false).frame(maxWidth: 280)
                        }
                    }
                    if !u.children.isEmpty {
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 8) {
                                ForEach(u.children) { c in box(c, compact: false).frame(width: 140) }
                            }
                        }
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func box(_ person: PersonRow?, compact: Bool) -> some View {
        if let p = person {
            Button { center(p.id) } label: {
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 4) {
                        LineageDot(lineage: p.lineage)
                        Text(compact ? given(p) : p.name).font(.caption).fontWeight(.semibold).lineLimit(compact ? 1 : 2)
                    }
                    if compact { Text(p.surname).font(.caption).fontWeight(.semibold).lineLimit(1) }
                    detail(p)
                }
                .frame(maxWidth: .infinity, minHeight: 56, alignment: .topLeading)
                .padding(8)
                .background(Theme.surface, in: RoundedRectangle(cornerRadius: 8))
                .overlay(RoundedRectangle(cornerRadius: 8).stroke(Theme.borderStrong))
            }
            .buttonStyle(.plain)
            .contextMenu {
                Button("Open Record", systemImage: "person.text.rectangle") { open(p.id) }
                Button("Center on This Person", systemImage: "scope") { center(p.id) }
            }
        } else {
            RoundedRectangle(cornerRadius: 8)
                .strokeBorder(Theme.borderStrong, style: StrokeStyle(lineWidth: 1, dash: [4, 3]))
                .frame(maxWidth: .infinity, minHeight: 56)
                .accessibilityHidden(true)
        }
    }

    private func detail(_ p: PersonRow) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            if !p.years.isEmpty { Text(p.years).monospacedDigit() }
            if !p.place.isEmpty { Text(p.place).lineLimit(1) }
        }
        .font(.caption).foregroundStyle(.secondary)
    }

    private func given(_ p: PersonRow) -> String {
        let given = p.name.replacingOccurrences(of: p.surname, with: "").trimmingCharacters(in: .whitespaces)
        return given.isEmpty ? p.name : given
    }
}

/// The “⋯” menu shared by the tabs: sync and the paired computer.
struct MoreMenu: View {
    let model: TreeModel
    var body: some View {
        Menu("More", systemImage: "ellipsis.circle") {
            Button("Sync Now", systemImage: "arrow.triangle.2.circlepath") {
                Task { await model.syncIfPaired() }
            }
            Button("Forget This Computer", systemImage: "link.badge.minus", role: .destructive) { model.unpair() }
        }
    }
}
