import Foundation
import Observation
import UIKit

/// State of the app: the paired computer, the last sync and the received tree.
@MainActor
@Observable
final class TreeModel {
    /// Address of the paired computer, as returned by `pair`.
    var desktop: String? = UserDefaults.standard.string(forKey: "desktop")
    var tree: Tree?
    var people: [PersonRow] = []
    var pairing = false
    var syncing = false
    var lastSync: Date? = UserDefaults.standard.object(forKey: "lastSync") as? Date
    var lastReport: SyncReport?
    /// The computer removed this device: it must be paired again.
    var removed = false
    var error: String?

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

    private func openTree() {
        let dir = dataDir.appendingPathComponent("tree").path
        guard FileManager.default.fileExists(atPath: dir + "/gedcom.ged") else { return }
        tree = try? Tree.open(treeDir: dir)
        people = tree?.individuals() ?? []
    }
}

extension PersonRow: Identifiable {}
