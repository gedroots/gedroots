import CoreSpotlight
import Foundation
import Observation
import UIKit
import UniformTypeIdentifiers

/// The app's tabs.
enum AppTab: Hashable {
    case tree, people, places, sources, media
}

/// A tree followed on this device: paired with the computer that shares it, kept in its own folder
/// (its own device key, so the computer can share several trees with the same device).
struct PairedTree: Codable, Identifiable, Hashable {
    /// Name of its folder, never shown.
    let id: String
    /// Address of the computer, as returned by `pair`.
    var desktop: String
    var name: String?
    var people: Int?
    var lastSync: Date?
    /// The computer removed this device: the tree must be paired again.
    var removed = false

    var title: String { name ?? String(localized: "Tree") }
}

/// State of the app: the trees followed, the one shown and its last sync.
@MainActor
@Observable
final class TreeModel {
    private(set) var trees: [PairedTree] = []
    /// The tree shown.
    private(set) var currentID: String?
    var tree: Tree?
    var people: [PersonRow] = []
    var places: [PlacePin] = []
    var sources: [SourceRow] = []
    /// Changes each time a tree is opened: views key their cached view models on it.
    var revision = 0
    var pairing = false
    var syncing = false
    var lastReport: SyncReport?
    var error: String?
    var tab = AppTab.tree
    /// The person the Tree tab is centred on; the root person when `nil`.
    var treeFocus: String?
    /// A record to open in the People tab (from Spotlight).
    var personToOpen: String?
    /// Changes when the app goes to another tab: open sheets and viewers close.
    var jumps = 0
    /// Add a Tree… is showing.
    var adding = false
    /// Manage Trees… is showing.
    var managing = false

    var current: PairedTree? { trees.first { $0.id == currentID } }
    var removed: Bool { current?.removed ?? false }
    var lastSync: Date? { current?.lastSync }

    private let dataDir: URL = {
        let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("GEDRoots", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }()

    init() {
        if let data = UserDefaults.standard.data(forKey: "trees"),
           let saved = try? JSONDecoder().decode([PairedTree].self, from: data) {
            trees = saved
        }
        migrateSingleTree()
        currentID = UserDefaults.standard.string(forKey: "currentTree") ?? trees.first?.id
        if current == nil { currentID = trees.first?.id }
        openTree()
    }

    private func folder(_ id: String) -> URL {
        dataDir.appendingPathComponent("trees", isDirectory: true).appendingPathComponent(id, isDirectory: true)
    }

    private func save() {
        if let data = try? JSONEncoder().encode(trees) { UserDefaults.standard.set(data, forKey: "trees") }
        UserDefaults.standard.set(currentID, forKey: "currentTree")
    }

    private func update(_ id: String, _ change: (inout PairedTree) -> Void) {
        guard let i = trees.firstIndex(where: { $0.id == id }) else { return }
        change(&trees[i])
        save()
    }

    /// Before several trees: one pairing, its key and tree in the app's folder itself.
    private func migrateSingleTree() {
        let defaults = UserDefaults.standard
        guard trees.isEmpty, let desktop = defaults.string(forKey: "desktop") else { return }
        let id = UUID().uuidString
        let target = folder(id)
        let fm = FileManager.default
        try? fm.createDirectory(at: target, withIntermediateDirectories: true)
        for name in ["endpoint.key", "blobs", "tree"] {
            try? fm.moveItem(at: dataDir.appendingPathComponent(name), to: target.appendingPathComponent(name))
        }
        trees = [PairedTree(id: id, desktop: desktop, lastSync: defaults.object(forKey: "lastSync") as? Date)]
        currentID = id
        save()
        defaults.removeObject(forKey: "desktop")
        defaults.removeObject(forKey: "lastSync")
        CSSearchableIndex.default().deleteSearchableItems(withDomainIdentifiers: ["people"]) { _ in }
    }

    /// Pairs a new tree, or the shown one again once its computer removed this device.
    func pair(with code: String) async {
        let code = code.trimmingCharacters(in: .whitespacesAndNewlines)
        guard isPairingCode(code: code) else {
            error = String(localized: "This is not a GEDRoots pairing code.")
            return
        }
        pairing = true
        defer { pairing = false }
        let again = adding ? nil : current.flatMap { $0.removed ? $0 : nil }
        let id = again?.id ?? UUID().uuidString
        let dir = folder(id)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let device = UIDevice.current
        do {
            let address = try await GEDRoots.pair(
                dataDir: dir.path, code: code, name: device.name, tablet: device.userInterfaceIdiom == .pad)
            if again != nil {
                update(id) { $0.desktop = address; $0.removed = false }
            } else {
                trees.append(PairedTree(id: id, desktop: address))
            }
            adding = false
            error = nil
            select(id)
        } catch {
            if again == nil { try? FileManager.default.removeItem(at: dir) }
            self.error = String(localized: "Pairing failed. The code may have expired: choose Add a Device again on your computer.")
        }
    }

    /// Shows another tree.
    func select(_ id: String) {
        guard trees.contains(where: { $0.id == id }) else { return }
        if id != currentID {
            currentID = id
            treeFocus = nil
            save()
            openTree()
        }
        Task { await syncIfPaired() }
    }

    /// Forgets a tree and its copy on this device.
    func remove(_ id: String) {
        trees.removeAll { $0.id == id }
        try? FileManager.default.removeItem(at: folder(id))
        CSSearchableIndex.default().deleteSearchableItems(withDomainIdentifiers: [Self.domain(id)]) { _ in }
        if currentID == id {
            currentID = trees.first?.id
            treeFocus = nil
            openTree()
        }
        save()
    }

    func syncIfPaired() async {
        guard let paired = current, !paired.removed, !syncing else { return }
        syncing = true
        defer { syncing = false }
        do {
            let report = try await sync(dataDir: folder(paired.id).path, desktop: paired.desktop)
            lastReport = report
            update(paired.id) { $0.lastSync = Date() }
            error = nil
            if paired.id == currentID { openTree() }
        } catch MobileError.Removed {
            update(paired.id) { $0.removed = true }
        } catch MobileError.NotShared {
            error = String(localized: "Your computer isn't sharing the tree right now.")
        } catch {
            self.error = String(localized: "Your computer can't be reached.")
        }
    }

    /// Shows the person in the Tree tab, centred on them.
    func showInTree(_ id: String) {
        treeFocus = id
        tab = .tree
        jumps += 1
    }

    /// Opens the person's record in the People tab.
    func openPerson(_ id: String) {
        personToOpen = id
        tab = .people
        jumps += 1
    }

    /// Opens a Spotlight result: the person, in their tree.
    func openSearchResult(_ identifier: String) {
        let parts = identifier.split(separator: "/", maxSplits: 1).map(String.init)
        guard parts.count == 2 else { return }
        select(parts[0])
        openPerson(parts[1])
    }

    private func openTree() {
        error = nil
        guard let paired = current else {
            tree = nil
            people = []
            places = []
            sources = []
            revision += 1
            return
        }
        let dir = folder(paired.id).appendingPathComponent("tree").path
        tree = FileManager.default.fileExists(atPath: dir + "/gedcom.ged") ? try? Tree.open(treeDir: dir) : nil
        people = tree?.individuals() ?? []
        places = tree?.places() ?? []
        sources = tree?.sources() ?? []
        revision += 1
        if let tree {
            let name = tree.name()
            let count = Int(tree.peopleCount())
            if paired.name != name || paired.people != count {
                update(paired.id) { $0.name = name ?? $0.name; $0.people = count }
            }
            Self.index(people, tree: paired.id)
        }
    }

    private static func domain(_ tree: String) -> String { "people.\(tree)" }

    /// Lists the individuals in the iPhone's search: a result opens their record. The index
    /// stays on the device.
    private static func index(_ people: [PersonRow], tree: String) {
        let items = people.map { p in
            let about = CSSearchableItemAttributeSet(contentType: .content)
            about.title = p.name
            about.contentDescription = [p.years, p.place].filter { !$0.isEmpty }.joined(separator: " · ")
            return CSSearchableItem(uniqueIdentifier: "\(tree)/\(p.id)", domainIdentifier: domain(tree), attributeSet: about)
        }
        let index = CSSearchableIndex.default()
        index.deleteSearchableItems(withDomainIdentifiers: [domain(tree)]) { _ in
            index.indexSearchableItems(items) { _ in }
        }
    }
}

extension PersonRow: Identifiable {}
