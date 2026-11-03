import CoreSpotlight
import Foundation
import Observation
import UIKit
import UniformTypeIdentifiers

/// The app's tabs.
enum AppTab: Hashable {
    case tree, people, places, sources, media
}

/// State of the app: the paired computer, the last sync and the received tree.
@MainActor
@Observable
final class TreeModel {
    /// Address of the paired computer, as returned by `pair`.
    var desktop: String? = UserDefaults.standard.string(forKey: "desktop")
    var tree: Tree?
    var people: [PersonRow] = []
    var places: [PlacePin] = []
    var sources: [SourceRow] = []
    /// Changes each time a tree is opened: views key their cached view models on it.
    var revision = 0
    var pairing = false
    var syncing = false
    var lastSync: Date? = UserDefaults.standard.object(forKey: "lastSync") as? Date
    var lastReport: SyncReport?
    /// The computer removed this device: it must be paired again.
    var removed = false
    var error: String?
    var tab = AppTab.tree
    /// The person the Tree tab is centred on; the root person when `nil`.
    var treeFocus: String?
    /// A record to open in the People tab (from Spotlight).
    var personToOpen: String?
    /// Changes when the app goes to another tab: open sheets and viewers close.
    var jumps = 0

    private let dataDir: URL = {
        let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("GEDRoots", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }()

    init() {
        openTree()
    }

    func pair(with code: String) async {
        let code = code.trimmingCharacters(in: .whitespacesAndNewlines)
        guard isPairingCode(code: code) else {
            error = String(localized: "This is not a GEDRoots pairing code.")
            return
        }
        pairing = true
        defer { pairing = false }
        let device = UIDevice.current
        do {
            let address = try await GEDRoots.pair(
                dataDir: dataDir.path, code: code, name: device.name, tablet: device.userInterfaceIdiom == .pad)
            desktop = address
            UserDefaults.standard.set(address, forKey: "desktop")
            removed = false
            error = nil
            await syncIfPaired()
        } catch {
            self.error = String(localized: "Pairing failed. The code may have expired: choose Add a Device again on your computer.")
        }
    }

    /// Forgets the computer; the received tree stays until another one is paired.
    func unpair() {
        desktop = nil
        removed = false
        UserDefaults.standard.removeObject(forKey: "desktop")
    }

    func syncIfPaired() async {
        guard let desktop, !syncing else { return }
        syncing = true
        defer { syncing = false }
        do {
            let report = try await sync(dataDir: dataDir.path, desktop: desktop)
            lastReport = report
            lastSync = Date()
            UserDefaults.standard.set(lastSync, forKey: "lastSync")
            error = nil
            openTree()
        } catch MobileError.Removed {
            removed = true
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

    private func openTree() {
        let dir = dataDir.appendingPathComponent("tree").path
        guard FileManager.default.fileExists(atPath: dir + "/gedcom.ged") else { return }
        tree = try? Tree.open(treeDir: dir)
        people = tree?.individuals() ?? []
        places = tree?.places() ?? []
        sources = tree?.sources() ?? []
        revision += 1
        Self.index(people)
    }

    /// Lists the individuals in the iPhone's search: a result opens their record. The index
    /// stays on the device.
    private static func index(_ people: [PersonRow]) {
        let items = people.map { p in
            let about = CSSearchableItemAttributeSet(contentType: .content)
            about.title = p.name
            about.contentDescription = [p.years, p.place].filter { !$0.isEmpty }.joined(separator: " · ")
            return CSSearchableItem(uniqueIdentifier: p.id, domainIdentifier: "people", attributeSet: about)
        }
        let index = CSSearchableIndex.default()
        index.deleteSearchableItems(withDomainIdentifiers: ["people"]) { _ in
            index.indexSearchableItems(items) { _ in }
        }
    }
}

extension PersonRow: Identifiable {}
