import CoreSpotlight
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
                    TabView(selection: $model.tab) {
                        TreeView(model: model)
                            .tabItem { Label("Tree", systemImage: "point.3.connected.trianglepath.dotted") }
                            .tag(AppTab.tree)
                        PeopleView(model: model)
                            .tabItem { Label("People", systemImage: "person.2") }
                            .tag(AppTab.people)
                        PlacesView(model: model)
                            .tabItem { Label("Places", systemImage: "mappin.and.ellipse") }
                            .tag(AppTab.places)
                        SourcesView(model: model)
                            .tabItem { Label("Sources", systemImage: "book") }
                            .tag(AppTab.sources)
                        MediaView(model: model)
                            .tabItem { Label("Media", systemImage: "photo.on.rectangle") }
                            .tag(AppTab.media)
                    }
                    .tint(Theme.accent)
                }
            }
            .task { await model.syncIfPaired() }
            .onContinueUserActivity(CSSearchableItemActionType) { activity in
                if let id = activity.userInfo?[CSSearchableItemActivityIdentifier] as? String {
                    model.openPerson(id)
                }
            }
            .onChange(of: scenePhase) { _, phase in
                if phase == .active { Task { await model.syncIfPaired() } }
            }
        }
    }
}
