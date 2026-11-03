import SwiftUI

/// The Sources tab: every source by title, searchable.
struct SourcesView: View {
    let model: TreeModel
    @State private var query = ""

    private var rows: [SourceRow] {
        let all = model.sources
        guard !query.isEmpty else { return all }
        return all.filter { $0.title.localizedStandardContains(query) || $0.detail.localizedStandardContains(query) }
    }

    var body: some View {
        NavigationStack {
            List(rows, id: \.id) { s in
                NavigationLink(value: SourceLink(id: s.id)) { SourceLabel(source: s) }
            }
            .overlay {
                if model.sources.isEmpty { ContentUnavailableView("No Sources", systemImage: "book.closed") }
            }
            .searchable(text: $query)
            .navigationTitle("Sources")
            .navigationDestination(for: SourceLink.self) { link in SourceView(model: model, id: link.id) }
            .navigationDestination(for: String.self) { id in PersonView(model: model, id: id) }
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

/// A source: title, repository and call number, web link, images, then the acts read in it, by
/// page, each with its events.
struct SourceView: View {
    let model: TreeModel
    let id: String
    @State private var openEvent: EventRef?
    @State private var viewing: MediaRow?

    var body: some View {
        if let s = model.tree?.source(id: id) {
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
                .listRowInsets(EdgeInsets(top: 8, leading: 0, bottom: 8, trailing: 0))
                if !s.media.isEmpty {
                    Section("Images") {
                        MediaStrip(media: s.media, open: { viewing = $0 })
                    }
                    .listRowInsets(EdgeInsets())
                    .listRowBackground(Color.clear)
                }
                ForEach(Array(s.acts.enumerated()), id: \.offset) { _, act in
                    Section(act.page.isEmpty ? String(localized: "Acts") : act.page) {
                        ForEach(act.events, id: \.event.id) { e in
                            Button { openEvent = e.event } label: {
                                VStack(alignment: .leading, spacing: 2) {
                                    HStack {
                                        Text(e.title)
                                        Spacer()
                                        Text(e.date).font(.footnote).foregroundStyle(.secondary).monospacedDigit()
                                    }
                                    let names = e.people.map(\.name).joined(separator: " & ")
                                    if !names.isEmpty { Text(names).font(.footnote).foregroundStyle(.secondary) }
                                }
                                .foregroundStyle(.primary)
                            }
                        }
                    }
                }
            }
            .navigationBarTitleDisplayMode(.inline)
            .sheet(item: $openEvent) { ref in
                EventSheet(model: model, ref: ref).presentationDetents([.medium, .large])
            }
            .fullScreenCover(item: $viewing) { m in ImageViewer(model: model, media: m) }
        } else {
            ContentUnavailableView("Not Found", systemImage: "book.closed")
        }
    }
}
