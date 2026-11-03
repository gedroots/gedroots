import MapKit
import SwiftUI

/// An event opened from a timeline: date, place with its pin, the people and their roles, cited
/// sources and the note. Read-only, in a sheet like Maps' place cards.
struct EventSheet: View {
    let model: TreeModel
    let ref: EventRef
    @Environment(\.dismiss) private var dismiss
    @State private var detail: EventDetail?
    @State private var loaded = false
    @State private var viewing: MediaRow?

    var body: some View {
        NavigationStack {
            if let e = detail {
                List {
                    Section {
                        if !e.date.isEmpty {
                            LabeledContent("Date") { Text(e.date).monospacedDigit() }
                        }
                        if let place = e.place { PlaceRow(place: place) }
                    }
                    if !e.principals.isEmpty {
                        Section(e.principals.count > 1 ? "Spouses" : "Person") { people(e.principals) }
                    }
                    if !e.participants.isEmpty {
                        Section("Present") {
                            ForEach(e.participants, id: \.person.id) { p in
                                NavigationLink(value: p.person.id) {
                                    LabeledContent { Text(p.role) } label: { Text(p.person.name) }
                                }
                            }
                        }
                    }
                    if !e.citations.isEmpty {
                        Section("Cited Sources") {
                            ForEach(Array(e.citations.enumerated()), id: \.offset) { _, c in
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(c.source.title)
                                    let detail = [c.source.detail, c.page].filter { !$0.isEmpty }.joined(separator: " · ")
                                    if !detail.isEmpty { Text(detail).font(.footnote).foregroundStyle(.secondary) }
                                }
                            }
                        }
                    }
                    if !e.media.isEmpty {
                        Section { MediaStrip(media: e.media, open: { viewing = $0 }) }
                            .listRowInsets(EdgeInsets())
                            .listRowBackground(Color.clear)
                    }
                    if let note = e.note {
                        Section("Note") { Text(note) }
                    }
                }
                .listSectionSpacing(.compact)
                .contentMargins(.top, 8, for: .scrollContent)
                .navigationTitle(e.title)
                .navigationBarTitleDisplayMode(.inline)
                .navigationDestination(for: String.self) { id in PersonView(model: model, id: id) }
                .toolbar {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button("Done") { dismiss() }
                    }
                }
                .mediaViewer(model: model, media: $viewing)
            } else if loaded {
                ContentUnavailableView("Not Found", systemImage: "calendar.badge.exclamationmark")
            }
        }
        .tint(Theme.accent)
        .onChange(of: model.jumps) { dismiss() }
        .task(id: "\(ref.id)#\(model.revision)") {
            detail = model.tree?.event(event: ref)
            loaded = true
        }
    }

    private func people(_ rows: [PersonRow]) -> some View {
        ForEach(rows) { p in
            NavigationLink(value: p.id) { PersonLabel(person: p) }
        }
    }
}

/// Place of an event, with a small map when it has coordinates.
struct PlaceRow: View {
    let place: PlaceRef
    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(place.short)
                if place.name != place.short {
                    Text(place.name).font(.footnote).foregroundStyle(.secondary)
                }
            }
            Spacer()
            if let lat = place.latitude, let lon = place.longitude {
                let pin = CLLocationCoordinate2D(latitude: lat, longitude: lon)
                Map(initialPosition: .region(MKCoordinateRegion(
                    center: pin, span: MKCoordinateSpan(latitudeDelta: 0.2, longitudeDelta: 0.2)))
                ) {
                    // The map names the town already: the pin goes without a title.
                    Marker(place.short, coordinate: pin).tint(Theme.accent).annotationTitles(.hidden)
                }
                .mapStyle(.standard(pointsOfInterest: .excludingAll))
                .frame(width: 72, height: 52)
                .clipShape(RoundedRectangle(cornerRadius: 8))
                .allowsHitTesting(false)
                .accessibilityHidden(true)
            }
        }
    }
}
