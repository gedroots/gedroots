import CoreLocation
import MapKit
import SwiftUI

/// The Places tab: a pin per place that has coordinates in the file (`MAP`), like Maps. A tap
/// on a pin opens the place's sheet with its events; the pins come from the file, so they show
/// even offline.
struct PlacesView: View {
    let model: TreeModel
    @State private var selection: String?
    @State private var scope = Scope.all
    @State private var query = ""
    /// Places matching the search; all of them when nothing is searched.
    @State private var matching: Set<String>?

    private var pins: [PlacePin] {
        model.places.filter { pin in
            (scope == .all || pin.ancestors) && (matching?.contains(pin.name) ?? true)
        }
    }

    var body: some View {
        NavigationStack {
            PlacesMap(pins: pins, selection: $selection)
            .ignoresSafeArea(edges: .bottom)
            .overlay {
                if model.places.isEmpty {
                    ContentUnavailableView("No Located Places", systemImage: "mappin.slash")
                }
            }
            .treeSearch(model: model, zone: .places, prompt: "Search Places", query: $query)
            // Like People: everyone's places, or only the ancestors'.
            .searchScopes($scope, activation: .onSearchPresentation) {
                Text("All").tag(Scope.all)
                Text("Ancestors").tag(Scope.ancestors)
            }
            .task(id: "\(query)#\(model.revision)") {
                matching = query.isEmpty ? nil : Set(model.tree?.searchPlaces(query: query) ?? [])
            }
            .navigationTitle("Places")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .topBarTrailing) { MoreMenu(model: model) } }
            .onChange(of: model.jumps) { selection = nil }
            .sheet(item: Binding(get: { selection.flatMap { s in pins.first { $0.name == s } } },
                                 set: { selection = $0?.name })) { pin in
                PlaceSheet(model: model, pin: pin)
                    .presentationDetents([.fraction(0.35), .large])
                    .presentationBackgroundInteraction(.enabled(upThrough: .fraction(0.35)))
            }
        }
        .tint(Theme.accent)
    }
}

extension PlacePin: Identifiable {
    public var id: String { name }
}

/// The map itself, in MapKit's own view: SwiftUI's `Map` can't cluster, and hundreds of places
/// in one region pile up. Pins merge into a count as they crowd; a tap on a group zooms in, a
/// tap on a place centres on it and opens its sheet.
struct PlacesMap: UIViewRepresentable {
    let pins: [PlacePin]
    @Binding var selection: String?

    func makeUIView(context: Context) -> MKMapView {
        let map = MKMapView()
        map.delegate = context.coordinator
        map.pointOfInterestFilter = .excludingAll
        // Under the floating tab bar it would be half hidden; the map is never rotated here anyway.
        map.showsCompass = false
        map.register(PlaceMarker.self, forAnnotationViewWithReuseIdentifier: MKMapViewDefaultAnnotationViewReuseIdentifier)
        map.register(GroupMarker.self, forAnnotationViewWithReuseIdentifier: MKMapViewDefaultClusterAnnotationViewReuseIdentifier)
        context.coordinator.attach(map)
        let locate = MKUserTrackingButton(mapView: map)
        locate.translatesAutoresizingMaskIntoConstraints = false
        locate.backgroundColor = .systemBackground
        locate.layer.cornerRadius = 8
        map.addSubview(locate)
        NSLayoutConstraint.activate([
            locate.trailingAnchor.constraint(equalTo: map.safeAreaLayoutGuide.trailingAnchor, constant: -16),
            locate.topAnchor.constraint(equalTo: map.safeAreaLayoutGuide.topAnchor, constant: 16),
        ])
        return map
    }

    func updateUIView(_ map: MKMapView, context: Context) {
        context.coordinator.parent = self
        let shown = map.annotations.compactMap { $0 as? PlaceAnnotation }
        let names = Set(pins.map(\.name))
        if Set(shown.map(\.pin.name)) != names {
            map.removeAnnotations(shown)
            map.addAnnotations(pins.map(PlaceAnnotation.init))
            // Once laid out: fitting a map that has no size yet leaves places outside.
            DispatchQueue.main.async {
                map.showAnnotations(map.annotations.filter { $0 is PlaceAnnotation }, animated: false)
            }
        }
        if selection == nil {
            map.selectedAnnotations.forEach { map.deselectAnnotation($0, animated: true) }
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(parent: self) }

    final class Coordinator: NSObject, MKMapViewDelegate, CLLocationManagerDelegate {
        var parent: PlacesMap
        /// Asks for the permission the first time the location button is tapped: the map alone
        /// never asks, and the button would wait for a location forever.
        private let location = CLLocationManager()
        private weak var map: MKMapView?

        init(parent: PlacesMap) { self.parent = parent }

        func attach(_ map: MKMapView) {
            self.map = map
            location.delegate = self
            map.showsUserLocation = Self.allowed(location.authorizationStatus)
        }

        private static func allowed(_ status: CLAuthorizationStatus) -> Bool {
            status == .authorizedWhenInUse || status == .authorizedAlways
        }

        func mapView(_ map: MKMapView, didChange mode: MKUserTrackingMode, animated: Bool) {
            guard mode != .none else { return }
            switch location.authorizationStatus {
            case .notDetermined: location.requestWhenInUseAuthorization()
            case .denied, .restricted:
                // Nothing to follow: the button goes back to rest instead of spinning.
                map.setUserTrackingMode(.none, animated: false)
                if let settings = URL(string: UIApplication.openSettingsURLString) {
                    UIApplication.shared.open(settings)
                }
            default: map.showsUserLocation = true
            }
        }

        func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
            guard let map else { return }
            if Self.allowed(manager.authorizationStatus) {
                map.showsUserLocation = true
            } else if manager.authorizationStatus != .notDetermined {
                map.setUserTrackingMode(.none, animated: false)
            }
        }

        func mapView(_ map: MKMapView, didSelect annotation: MKAnnotation) {
            if let group = annotation as? MKClusterAnnotation {
                map.deselectAnnotation(group, animated: false)
                map.showAnnotations(group.memberAnnotations, animated: true)
            } else if let place = annotation as? PlaceAnnotation {
                map.setCenter(place.coordinate, animated: true)
                parent.selection = place.pin.name
            }
        }
    }
}

final class PlaceAnnotation: NSObject, MKAnnotation {
    let pin: PlacePin
    init(_ pin: PlacePin) { self.pin = pin }
    var coordinate: CLLocationCoordinate2D { CLLocationCoordinate2D(latitude: pin.latitude, longitude: pin.longitude) }
    var title: String? { pin.short }
}

/// A place: the number of its events in the pin, the accent when ancestors lived there.
final class PlaceMarker: MKMarkerAnnotationView {
    override var annotation: MKAnnotation? {
        didSet {
            guard let place = annotation as? PlaceAnnotation else { return }
            clusteringIdentifier = "place"
            displayPriority = place.pin.ancestors ? .defaultHigh : .defaultLow
            glyphText = place.pin.events > 999 ? "999+" : "\(place.pin.events)"
            markerTintColor = place.pin.ancestors ? UIColor(Theme.accent) : .systemGray
        }
    }
}

/// Places grouped as they crowd: how many, in the accent when one of them is an ancestor's.
final class GroupMarker: MKMarkerAnnotationView {
    override var annotation: MKAnnotation? {
        didSet {
            guard let group = annotation as? MKClusterAnnotation else { return }
            let places = group.memberAnnotations.compactMap { $0 as? PlaceAnnotation }
            glyphText = "\(places.count)"
            titleVisibility = .hidden
            subtitleVisibility = .hidden
            markerTintColor = places.contains { $0.pin.ancestors } ? UIColor(Theme.accent) : .systemGray
            displayPriority = .defaultHigh
        }
    }
}

/// A place's sheet: its name, then its events by date.
struct PlaceSheet: View {
    let model: TreeModel
    let pin: PlacePin
    @State private var openEvent: EventRef?
    @State private var events: [PlaceEvent] = []

    var body: some View {
        NavigationStack {
            List {
                ForEach(events, id: \.event.id) { e in
                    Button { openEvent = e.event } label: {
                        HStack(alignment: .firstTextBaseline, spacing: 12) {
                            Text(e.year).font(.footnote).foregroundStyle(Color.secondary).monospacedDigit()
                                .frame(width: 40, alignment: .leading)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(e.title)
                                let names = e.people.map(\.name).joined(separator: " & ")
                                if !names.isEmpty { Text(names).font(.footnote).foregroundStyle(Color.secondary) }
                            }
                        }
                        .foregroundStyle(Color.primary)
                    }
                }
            }
            .contentMargins(.top, 8, for: .scrollContent)
            .navigationTitle(pin.short)
            .navigationBarTitleDisplayMode(.inline)
            .sheet(item: $openEvent) { ref in
                EventSheet(model: model, ref: ref).presentationDetents([.medium, .large])
            }
        }
        .task(id: "\(pin.name)#\(model.revision)") { events = model.tree?.placeEvents(name: pin.name) ?? [] }
        .onChange(of: model.jumps) { openEvent = nil }
        // Opaque: the map behind is too busy for text over glass.
        .presentationBackground(Color(UIColor.systemGroupedBackground))
    }
}
