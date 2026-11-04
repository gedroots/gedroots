import SwiftUI
import VisionKit

/// Pairing with the computer: the steps to show the code there, then scan it (or paste it).
struct PairView: View {
    let model: TreeModel
    @State private var scanning = false
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                if model.removed, !model.adding, let tree = model.current {
                    Section {
                        Label("This device was removed from your computer.", systemImage: "link.badge.minus")
                        Button("Remove This Tree", systemImage: "trash", role: .destructive) { model.remove(tree.id) }
                    }
                }
                Section {
                    Step(number: 1, text: "Open your tree in GEDRoots on your computer.")
                    Step(number: 2, text: "Choose File › Mobile Devices…")
                    Step(number: 3, text: "Choose Add a Device, then scan the code.")
                }
                Section {
                    Button("Scan the Code", systemImage: "qrcode.viewfinder") { scanning = true }
                        .disabled(!DataScannerViewController.isSupported || model.pairing)
                    PasteButton(payloadType: String.self) { strings in
                        if let code = strings.first { Task { await model.pair(with: code) } }
                    }
                    .disabled(model.pairing)
                } footer: {
                    if model.pairing {
                        Label("Pairing…", systemImage: "link")
                    } else if let error = model.error {
                        Text(error).foregroundStyle(.red)
                    }
                }
            }
            .navigationTitle(model.adding ? "Add a Tree" : "Pair With Your Computer")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                if model.adding {
                    ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                }
            }
            .sheet(isPresented: $scanning) {
                QRScanner { code in
                    scanning = false
                    Task { await model.pair(with: code) }
                }
                .ignoresSafeArea()
            }
        }
    }
}

private struct Step: View {
    let number: Int
    let text: LocalizedStringKey
    var body: some View {
        Label { Text(text) } icon: { Image(systemName: "\(number).circle.fill").foregroundStyle(.tint) }
    }
}

/// VisionKit's data scanner, limited to QR codes.
struct QRScanner: UIViewControllerRepresentable {
    let found: (String) -> Void

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let scanner = DataScannerViewController(
            recognizedDataTypes: [.barcode(symbologies: [.qr])],
            isHighlightingEnabled: true
        )
        scanner.delegate = context.coordinator
        try? scanner.startScanning()
        return scanner
    }

    func updateUIViewController(_ controller: DataScannerViewController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(found: found) }

    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let found: (String) -> Void
        private var done = false
        init(found: @escaping (String) -> Void) { self.found = found }

        func dataScanner(_ scanner: DataScannerViewController, didAdd items: [RecognizedItem], allItems: [RecognizedItem]) {
            for case let .barcode(code) in items {
                if !done, let value = code.payloadStringValue, value.hasPrefix("gedroots:") {
                    done = true
                    scanner.stopScanning()
                    found(value)
                }
            }
        }
    }
}
