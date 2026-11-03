import SwiftUI
import UIKit

/// GEDRoots tokens (design.md, crates/gedroots/src/theme.rs), light and dark.
enum Theme {
    static let accent = dynamic(0x6D3DF0, 0xB997FF)
    static let accentSoft = Color(UIColor { $0.userInterfaceStyle == .dark
        ? UIColor(rgb: 0xB997FF).withAlphaComponent(0.16)
        : UIColor(rgb: 0x6D3DF0).withAlphaComponent(0.10) })
    static let paternal = dynamic(0x3F5BD8, 0x8FA2FF)
    static let maternal = dynamic(0x9C36B5, 0xE39BF0)
    static let surface = dynamic(0xF4EFE6, 0x222222)
    static let hover = dynamic(0xEEE7DA, 0x2B2B2B)
    static let borderStrong = dynamic(0xD5CAB7, 0x3D3D3D)
    static let viewer = Color(UIColor(rgb: 0x0E0E0E))

    static func color(_ lineage: Lineage) -> Color {
        switch lineage {
        case .root: accent
        case .paternal: paternal
        case .maternal: maternal
        case .other: .clear
        }
    }

    private static func dynamic(_ light: UInt32, _ dark: UInt32) -> Color {
        Color(UIColor { $0.userInterfaceStyle == .dark ? UIColor(rgb: dark) : UIColor(rgb: light) })
    }
}

extension UIColor {
    convenience init(rgb: UInt32) {
        self.init(
            red: CGFloat((rgb >> 16) & 0xFF) / 255, green: CGFloat((rgb >> 8) & 0xFF) / 255,
            blue: CGFloat(rgb & 0xFF) / 255, alpha: 1)
    }
}

/// Lineage dot: the root person and their ancestors, by side; never colour alone, the Sosa number
/// is in the record.
struct LineageDot: View {
    let lineage: Lineage
    var body: some View {
        Circle().fill(Theme.color(lineage)).frame(width: 8, height: 8).accessibilityHidden(true)
    }
}
