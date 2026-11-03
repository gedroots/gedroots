import SwiftUI

/// The Tree tab: the grandparents, the parents, the person (the root person at first), then their
/// unions and children. A tap on a relative centres the tree on them; a tap on the person opens
/// their record. Lineages are told by a dot and by their place in the tree, never colour alone.
struct TreeView: View {
    let model: TreeModel
    @State private var path = NavigationPath()
    @State private var family: FamilyView?

    private var current: String? { model.treeFocus ?? model.tree?.root() }

    var body: some View {
        NavigationStack(path: $path) {
            Group {
                if let f = family {
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
                    Button("Root Person", systemImage: "house") { withAnimation { model.treeFocus = nil } }
                        .disabled(model.treeFocus == nil)
                }
                ToolbarItem(placement: .topBarTrailing) { MoreMenu(model: model) }
            }
        }
        .tint(Theme.accent)
        .task(id: "\(current ?? "")#\(model.revision)") { family = current.flatMap { model.tree?.family(id: $0) } }
        // Show in Tree from a record: back to the chart, centred on the person.
        .onChange(of: model.jumps) { if model.tab == .tree { path = NavigationPath() } }
    }

    private func center(_ id: String) {
        withAnimation(.snappy(duration: 0.2)) { model.treeFocus = id }
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
                ForEach(0..<4, id: \.self) { i in box(family.grandparents[i], compact: true).card("g\(i)") }
            }
            HStack(spacing: 8) {
                ForEach(0..<2, id: \.self) { i in box(family.parents[i], compact: false).card("p\(i)") }
            }
            Button { open(family.person.id) } label: {
                HStack(spacing: 12) {
                    Portrait(path: family.portrait, crop: family.portraitCrop, name: family.person.name, size: 56)
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
            .card("me")
            ForEach(Array(family.unions.enumerated()), id: \.offset) { u, union in
                VStack(spacing: 24) {
                    if let s = union.spouse {
                        VStack(spacing: 4) {
                            if !union.married.isEmpty {
                                Text("⚭ \(union.married)").font(.footnote).foregroundStyle(.secondary).monospacedDigit()
                            }
                            box(s, compact: false).frame(maxWidth: 280).card("s\(u)")
                        }
                    }
                    if !union.children.isEmpty {
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 8) {
                                ForEach(Array(union.children.enumerated()), id: \.offset) { i, c in
                                    box(c, compact: false).frame(width: 140).card("c\(u)-\(i)")
                                }
                            }
                        }
                        .scrollClipDisabled()
                    }
                }
            }
        }
        .backgroundPreferenceValue(CardBounds.self) { cards in
            GeometryReader { proxy in Connectors(family: family, cards: cards.mapValues { proxy[$0] }) }
        }
    }

    @ViewBuilder
    private func box(_ person: PersonRow?, compact: Bool) -> some View {
        if let p = person {
            Button { center(p.id) } label: {
                VStack(alignment: .leading, spacing: 2) {
                    // Grandparents: the given names on two lines at most, one word per line so that a
                    // long name shrinks a little rather than breaking in the middle.
                    let words = compact ? given(p).split(separator: " ").map(String.init) : [p.name]
                    HStack(alignment: .firstTextBaseline, spacing: 4) {
                        if p.lineage != .other { LineageDot(lineage: p.lineage) }
                        Text(words.first ?? "").font(.caption).fontWeight(.semibold)
                            .lineLimit(compact ? 1 : 2).minimumScaleFactor(compact ? 0.75 : 1)
                    }
                    if compact, words.count > 1 {
                        Text(words.dropFirst().joined(separator: " ")).font(.caption).fontWeight(.semibold)
                            .lineLimit(1).minimumScaleFactor(0.75)
                    }
                    if compact {
                        Text(p.surname).font(.caption).fontWeight(.semibold).lineLimit(1).minimumScaleFactor(0.8)
                        if !p.years.isEmpty {
                            Text(p.years).font(.caption2).foregroundStyle(.secondary).monospacedDigit().lineLimit(1)
                                .minimumScaleFactor(0.8)
                        }
                    } else {
                        detail(p)
                    }
                }
                .frame(maxWidth: .infinity, minHeight: compact ? 72 : 56, alignment: .topLeading)
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

/// Where each card of the chart is (`g0`…`g3`, `p0`, `p1`, `me`, `s0`, `c0-0`…).
struct CardBounds: PreferenceKey {
    static let defaultValue: [String: Anchor<CGRect>] = [:]
    static func reduce(value: inout [String: Anchor<CGRect>], nextValue: () -> [String: Anchor<CGRect>]) {
        value.merge(nextValue()) { $1 }
    }
}

extension View {
    fileprivate func card(_ key: String) -> some View {
        anchorPreference(key: CardBounds.self, value: .bounds) { [key: $0] }
    }
}

/// Lines between the generations, behind the cards: each couple joins above their child, in the
/// colour of the lineage (father's side, mother's side, the person), neutral for the descendants.
struct Connectors: View {
    let family: FamilyView
    let cards: [String: CGRect]

    var body: some View {
        ZStack {
            join(["g0", "g1"], to: "p0").stroke(Theme.paternal, lineWidth: 1.5)
            join(["g2", "g3"], to: "p1").stroke(Theme.maternal, lineWidth: 1.5)
            join(["p0"], to: "me", drop: false).stroke(Theme.paternal, lineWidth: 1.5)
            join(["p1"], to: "me", drop: false).stroke(Theme.maternal, lineWidth: 1.5)
            join(["p0", "p1"], to: "me", parents: false).stroke(Theme.accent, lineWidth: 1.5)
            descendants.stroke(Theme.borderStrong, lineWidth: 1.5)
        }
        .accessibilityHidden(true)
    }

    /// From the bottom of `from` down to a bar halfway, then down to the top of `to`. `parents`:
    /// draw the parents' legs and the bar; `drop`: the leg down to the child.
    private func join(_ from: [String], to: String, parents: Bool = true, drop: Bool = true) -> Path {
        Path { path in
            let tops = from.compactMap { cards[$0] }
            guard let child = cards[to], !tops.isEmpty else { return }
            let bar = (tops.map(\.maxY).max()! + child.minY) / 2
            if parents {
                for t in tops {
                    path.move(to: CGPoint(x: t.midX, y: t.maxY))
                    path.addLine(to: CGPoint(x: t.midX, y: bar))
                    path.addLine(to: CGPoint(x: child.midX, y: bar))
                }
            }
            if drop {
                path.move(to: CGPoint(x: child.midX, y: bar))
                path.addLine(to: CGPoint(x: child.midX, y: child.minY))
            }
        }
    }

    /// The person to each spouse, and each couple (or the person alone) to their children.
    private var descendants: Path {
        Path { path in
            guard let me = cards["me"] else { return }
            for (u, union) in family.unions.enumerated() {
                let spouse = cards["s\(u)"]
                if let spouse {
                    path.move(to: CGPoint(x: me.midX, y: me.maxY))
                    path.addLine(to: CGPoint(x: spouse.midX, y: spouse.minY))
                }
                let parent = spouse ?? me
                let children = union.children.indices.compactMap { cards["c\(u)-\($0)"] }
                guard let first = children.first else { continue }
                let bar = (parent.maxY + first.minY) / 2
                path.move(to: CGPoint(x: parent.midX, y: parent.maxY))
                path.addLine(to: CGPoint(x: parent.midX, y: bar))
                for c in children {
                    path.move(to: CGPoint(x: min(parent.midX, c.midX), y: bar))
                    path.addLine(to: CGPoint(x: max(parent.midX, c.midX), y: bar))
                    path.move(to: CGPoint(x: c.midX, y: bar))
                    path.addLine(to: CGPoint(x: c.midX, y: c.minY))
                }
            }
        }
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
