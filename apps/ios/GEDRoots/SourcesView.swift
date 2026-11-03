import SwiftUI

/// The Sources tab: every source by title, searchable.
struct SourcesView: View {
    let model: TreeModel
    @State private var query = ""
    @State private var found: [SourceRow]?

    private var rows: [SourceRow] { found ?? model.sources }

    var body: some View {
        NavigationStack {
            List(rows, id: \.id) { s in
                NavigationLink(value: SourceLink(id: s.id)) { SourceLabel(source: s) }
            }
            .overlay {
                if model.sources.isEmpty {
                    ContentUnavailableView("No Sources", systemImage: "book.closed")
                } else if rows.isEmpty {
                    ContentUnavailableView.search
                }
            }
            .treeSearch(model: model, zone: .sources, prompt: "Search Sources", query: $query)
            .task(id: "\(query)#\(model.revision)") {
                found = query.isEmpty ? nil : model.tree?.searchSources(query: query)
            }
            .navigationTitle("Sources")
            .navigationDestination(for: SourceLink.self) { link in SourceView(model: model, id: link.id) }
            .navigationDestination(for: PersonLink.self) { link in PersonView(model: model, id: link.id) }
            .refreshable { await model.syncIfPaired() }
            .toolbar { ToolbarItem(placement: .topBarTrailing) { MoreMenu(model: model) } }
        }
        .tint(Theme.accent)
    }
}

/// Navigation value of a source (people navigate with their bare identifier).
struct SourceLink: Hashable {
    let id: String
}

/// A person pushed on a navigation stack. Each push is its own value: the same person twice on a
/// stack (father → son → father) makes SwiftUI bounce between the two records.
struct PersonLink: Hashable {
    let id: String
    private let push = UUID()

    init(id: String) { self.id = id }
}

/// A source: title, repository and call number, web link, images, then the acts read in it, by
/// page, each with its events.
struct SourceView: View {
    let model: TreeModel
    let id: String
    @State private var openEvent: EventRef?
    @State private var viewing: MediaRow?
    @State private var detail: SourceDetail?
    @Namespace private var zoom
    @State private var loaded = false

    var body: some View {
        content.task(id: "\(id)#\(model.revision)") {
            detail = model.tree?.source(id: id)
            loaded = true
        }
    }

    @ViewBuilder private var content: some View {
        if let s = detail {
            List {
                Section {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(s.row.title).font(.title3).fontWeight(.semibold)
                        if !s.row.detail.isEmpty { Text(s.row.detail).font(.footnote).foregroundStyle(.secondary) }
                        if let author = s.author { Text(author).font(.footnote).foregroundStyle(.secondary) }
                        if let url = s.url.flatMap(URL.init(string:)) {
                            Link(destination: url) {
                                Label("Open the Web Page", systemImage: "arrow.up.right.square")
                            }
                            .font(.subheadline)
                            .padding(.top, 4)
                        }
                    }
                }
                .listRowBackground(Color.clear)
                if !s.media.isEmpty {
                    Section("Images") {
                        // Insets on the row only: on the section they would move its header too.
                        MediaStrip(media: s.media, zoom: zoom, open: { viewing = $0 })
                            .listRowInsets(EdgeInsets())
                            .listRowBackground(Color.clear)
                    }
                }
                if !s.acts.isEmpty {
                    Section("Acts") {
                        ForEach(Array(s.acts.enumerated()), id: \.offset) { _, act in
                            ForEach(act.events, id: \.event.id) { e in
                                Button { openEvent = e.event } label: { ActLabel(event: e, page: act.page) }
                            }
                        }
                    }
                }
            }
            .navigationBarTitleDisplayMode(.inline)
            .sheet(item: $openEvent) { ref in
                EventSheet(model: model, ref: ref).presentationDetents([.medium, .large])
            }
            .mediaViewer(model: model, media: $viewing, zoom: zoom)
            .onChange(of: model.jumps) {
                openEvent = nil
                viewing = nil
            }
        } else if loaded {
            ContentUnavailableView("Not Found", systemImage: "book.closed")
        }
    }
}

/// An event read in a source: what, when and on which page, then who.
struct ActLabel: View {
    let event: PlaceEvent
    let page: String
    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .firstTextBaseline) {
                Text(event.title)
                Spacer()
                let detail = [event.date, page].filter { !$0.isEmpty }.joined(separator: " · ")
                Text(detail).font(.footnote).foregroundStyle(Color.secondary).monospacedDigit()
            }
            let names = event.people.map(\.name).joined(separator: " & ")
            if !names.isEmpty { Text(names).font(.footnote).foregroundStyle(Color.secondary) }
        }
        .foregroundStyle(Color.primary)
    }
}
