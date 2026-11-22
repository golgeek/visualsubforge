import CoreGraphics
import Foundation
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
guard let window = windows.first(where: { $0[kCGWindowOwnerName as String] as? String == "Finder" && $0[kCGWindowName as String] as? String == "VisualSubForge" }), let id = window[kCGWindowNumber as String] as? Int else { exit(1) }
print(id)
