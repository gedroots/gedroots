import MapKit
import SwiftUI

/// The Places tab: a pin per place that has coordinates in the file (`MAP`), like Maps. A tap
/// on a pin opens the place's sheet with its events; the pins come from the file, so they show
/// even offline.
struct PlacesView: View {
    let model: TreeModel
    @State private var selection: String?
    @State private var ancestorsOnly = false
    @State private var query = ""
    @State private var position: MapCameraPosition = .automatic

    private var pins: [PlacePin] {
        let all = model.places
        return all.filter { pin in
            (!ancestorsOnly || pin.ancestors)
                && (query.isEmpty || pin.name.localizedStandardContains(query))
        }
    }

    var body: some View {
        NavigationStack {
            Map(position: $position, selection: $selection) {
                ForEach(pins, id: \.name) { pin in
                    Marker(pin.short, monogram: Text(verbatim: "\(pin.events)"),
                           coordinate: CLLocationCoordinate2D(latitude: pin.latitude, longitude: pin.longitude))
                        .tint(pin.ancestors ? Theme.accent : .gray)
                        .tag(pin.name)
                }
                UserAnnotation()
            }
            .mapControls {
                MapUserLocationButton()
                MapCompass()
            }
            .overlay {
                if model.places.isEmpty {
                    ContentUnavailableView("No Located Places", systemImage: "mappin.slash")
                }
            }
            .safeAreaInset(edge: .top) {
                HStack {
                    Toggle("Ancestors", isOn: $ancestorsOnly).toggleStyle(.button).buttonStyle(.bordered)
                    Spacer()
                }
                .padding(.horizontal, 16)
            }
            .searchable(text: $query, prompt: Text("Search Places"))
            .navigationTitle("Places")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .topBarTrailing) { MoreMenu(model: model) } }
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

/// A place's sheet: its name, then its events by date.
struct PlaceSheet: View {
    let model: TreeModel
    let pin: PlacePin
    @State private var openEvent: EventRef?

    var body: some View {
        NavigationStack {
            List {
                ForEach(model.tree?.placeEvents(name: pin.name) ?? [], id: \.event.id) { e in
                    Button { openEvent = e.event } label: {
                        HStack(alignment: .firstTextBaseline, spacing: 12) {
                            Text(e.year).font(.footnote).foregroundStyle(.secondary).monospacedDigit()
                                .frame(width: 40, alignment: .leading)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(e.title)
                                let names = e.people.map(\.name).joined(separator: " & ")
                                if !names.isEmpty { Text(names).font(.footnote).foregroundStyle(.secondary) }
                            }
                        }
                        .foregroundStyle(.primary)
                    }
                }
            }
            .navigationTitle(pin.short)
            .navigationBarTitleDisplayMode(.inline)
            .sheet(item: $openEvent) { ref in
                EventSheet(model: model, ref: ref).presentationDetents([.medium, .large])
            }
        }
    }
}
