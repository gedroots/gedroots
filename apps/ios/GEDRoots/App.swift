import SwiftUI

@main
struct GEDRootsApp: App {
    @State private var model = TreeModel()
    @Environment(\.scenePhase) private var scenePhase

    init() {
        setLanguage(language: Locale.current.language.languageCode?.identifier == "fr" ? "fr" : "en")
    }

    var body: some Scene {
        WindowGroup {
            Group {
                if model.desktop == nil || model.removed {
                    PairView(model: model)
                } else {
                    PeopleView(model: model)
                }
            }
            .task { await model.syncIfPaired() }
            .onChange(of: scenePhase) { _, phase in
                if phase == .active { Task { await model.syncIfPaired() } }
            }
        }
    }
}
