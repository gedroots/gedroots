import SwiftUI

extension SearchToken: Identifiable {
    public var id: String { criterion }
}

extension View {
    /// The search field of a tab, the same in every tab, with the desktop's criteria: they are
    /// tokens that read as phrases (“Born 1850–1900”), picked from suggestions made of the file's
    /// values. Nobody has to type the criteria syntax, though it still works. `query` receives
    /// the whole query of the zone.
    /// `onClose`: the search was closed (Cancel), e.g. to clear a scope that only shows while
    /// searching.
    /// `scoped`: a search scope narrows the results.
    func treeSearch(model: TreeModel, zone: SearchZone, prompt: LocalizedStringKey, query: Binding<String>,
                    scoped: Bool = false, onClose: @escaping () -> Void = {}) -> some View {
        modifier(TreeSearch(model: model, zone: zone, prompt: prompt, query: query, scoped: scoped, onClose: onClose))
    }
}

private struct TreeSearch: ViewModifier {
    let model: TreeModel
    let zone: SearchZone
    let prompt: LocalizedStringKey
    @Binding var query: String
    let scoped: Bool
    let onClose: () -> Void
    @State private var presented = false
    @State private var text = ""
    @State private var tokens: [SearchToken] = []
    @State private var suggestions: [SearchSuggestion] = []

    func body(content: Content) -> some View {
        content
            .searchable(text: $text, tokens: $tokens, isPresented: $presented, prompt: Text(prompt)) { token in
                Text(token.label)
            }
            .searchSuggestions {
                ForEach(groups, id: \.0) { group, items in
                    Section {
                        ForEach(items, id: \.token.criterion) { s in
                            Button { pick(s) } label: { Text(s.label).foregroundStyle(Color.primary) }
                        }
                    } header: {
                        Text(group)
                    }
                }
            }
            .onChange(of: text) {
                promote()
                publish()
            }
            .onChange(of: tokens) { publish() }
            .onChange(of: presented) {
                guard !presented else { return }
                // Closed: nothing filters what the tab shows any more.
                tokens = []
                text = ""
                onClose()
            }
            .task(id: "\(text)#\(tokens.map(\.criterion))#\(scoped)#\(model.revision)") { await suggest() }
    }

    /// Suggestions under their headings, in the order they come.
    private var groups: [(String, [SearchSuggestion])] {
        var out: [(String, [SearchSuggestion])] = []
        for s in suggestions {
            if let i = out.firstIndex(where: { $0.0 == s.group }) {
                out[i].1.append(s)
            } else {
                out.append((s.group, [s]))
            }
        }
        return out
    }

    private func pick(_ s: SearchSuggestion) {
        if !tokens.contains(s.token) { tokens.append(s.token) }
        text = s.rest
    }

    /// A criterion typed in full, once followed by a space, becomes a token too.
    private func promote() {
        guard text.last?.isWhitespace == true, let typed = model.tree?.typedCriteria(zone: zone, text: text),
              !typed.tokens.isEmpty
        else { return }
        tokens += typed.tokens.filter { !tokens.contains($0) }
        text = typed.rest
    }

    private func publish() {
        query = (tokens.map(\.criterion) + [text]).joined(separator: " ").trimmingCharacters(in: .whitespaces)
    }

    private func suggest() async {
        guard let tree = model.tree else {
            suggestions = []
            return
        }
        let zone = zone
        let criteria = tokens.map(\.criterion)
        let text = text
        // The zone's criteria show on an empty field only before anything narrows the results:
        // afterwards, the results show.
        if text.isEmpty && (!criteria.isEmpty || scoped) {
            suggestions = []
            return
        }
        // Typing on: this call is cancelled before it searches.
        try? await Task.sleep(for: .milliseconds(120))
        if Task.isCancelled { return }
        let found = await Task.detached { tree.suggestions(zone: zone, tokens: criteria, text: text) }.value
        if !Task.isCancelled { suggestions = found }
    }
}
