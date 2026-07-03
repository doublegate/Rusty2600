import Foundation

/// The `v2.11.0` "Field Trip" on-device save-state slot store -- the iOS
/// counterpart to `android/`'s `SaveSlots.kt`, mirroring the same
/// `rusty2600-frontend` manual save-state slot convention
/// (`crates/rusty2600-frontend/src/config.rs`, `v2.4.0` "Save Point"):
/// `slotCount` numbered slots (`0..7`), one file per slot named
/// `slot_<N>.<extension>`, all of one ROM's slots keyed under a directory
/// named after that ROM's identity tag so two different ROMs' slots can
/// never collide or be silently cross-loaded.
///
/// Uses `URL.applicationSupportDirectory` -- a genuinely private, per-app
/// location matching Android's `Context.filesDir` and desktop's
/// `directories::ProjectDirs::data_dir()`. NOT `.documentDirectory` (PR #21
/// bot review, Copilot): on iOS, `Documents/` is user-visible via the Files
/// app / iTunes-style file sharing (depending on entitlements) and is meant
/// for user-facing documents, which contradicts this module's own stated
/// "private, per-app save-data location" intent -- `Application Support` is
/// the location Apple's own docs recommend for exactly this kind of
/// internal, non-user-facing app data.
enum SaveSlots {
    /// Matches `rusty2600_frontend::config::SAVE_SLOT_COUNT` and
    /// `android/`'s `SaveSlots.SLOT_COUNT`.
    static let slotCount = 8

    /// Matches `rusty2600_frontend::config`'s `.r26s` slot-file extension.
    private static let fileExtension = "r26s"

    /// One save-state slot's on-disk status, probed fresh from the filesystem.
    struct SlotInfo {
        let slot: Int
        let exists: Bool
        let modified: Date?

        /// A human-readable label, e.g. `"Slot 3 (empty)"` or a slot with a
        /// real save shown as `"Slot 3 -- 2026-07-02 14:03:07 UTC"` -- the
        /// exact wording `rusty2600_frontend::shell::SaveSlotInfo::label`
        /// uses on desktop (and `android/`'s `SaveSlots.SlotInfo.label()`
        /// mirrors), so the mobile UX reads as the same feature everywhere.
        var label: String {
            guard let modified else { return "Slot \(slot) (empty)" }
            return "Slot \(slot) -- \(Self.formatter.string(from: modified)) UTC"
        }

        private static let formatter: DateFormatter = {
            let f = DateFormatter()
            f.dateFormat = "yyyy-MM-dd HH:mm:ss"
            f.timeZone = TimeZone(identifier: "UTC")
            f.locale = Locale(identifier: "en_US_POSIX")
            return f
        }()
    }

    // `URL.applicationSupportDirectory` (the modern iOS-16+ static property, PR #21 bot review,
    // Gemini Code Assist) rather than querying `FileManager` and force-indexing the result array.
    private static var appSupportDirectory: URL {
        .applicationSupportDirectory
    }

    private static func slotDirectory(romTag: UInt64) -> URL {
        appSupportDirectory
            .appendingPathComponent("saves", isDirectory: true)
            .appendingPathComponent(String(format: "%016llx", romTag), isDirectory: true)
    }

    /// The path to one save-state slot file (`<slot-dir>/slot_<N>.r26s`).
    static func slotFile(romTag: UInt64, slot: Int) -> URL {
        slotDirectory(romTag: romTag).appendingPathComponent("slot_\(slot).\(fileExtension)")
    }

    /// Probes `slot`'s current on-disk status for `romTag`.
    ///
    /// `url.resourceValues(forKeys:)` (PR #21 bot review, Gemini Code Assist) rather than
    /// `FileManager.attributesOfItem(atPath:)` -- the modern, `URL`-native way to read a
    /// modification date without going through the deprecated `URL.path` string bridge.
    static func probe(romTag: UInt64, slot: Int) -> SlotInfo {
        let url = slotFile(romTag: romTag, slot: slot)
        guard let values = try? url.resourceValues(forKeys: [.contentModificationDateKey]) else {
            return SlotInfo(slot: slot, exists: false, modified: nil)
        }
        return SlotInfo(slot: slot, exists: true, modified: values.contentModificationDate)
    }

    /// Probes all `slotCount` slots for `romTag`, in slot order.
    static func probeAll(romTag: UInt64) -> [SlotInfo] {
        (0..<slotCount).map { probe(romTag: romTag, slot: $0) }
    }

    /// Writes `data` (a `MobileEmulator.saveState()` blob) into `slot`,
    /// creating the ROM's slot directory if needed.
    static func save(romTag: UInt64, slot: Int, data: Data) throws {
        let dir = slotDirectory(romTag: romTag)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try data.write(to: slotFile(romTag: romTag, slot: slot), options: .atomic)
    }

    /// Reads `slot`'s blob back, or `nil` if the slot is empty. Pass the
    /// result straight to `MobileEmulator.loadState`.
    static func load(romTag: UInt64, slot: Int) -> Data? {
        try? Data(contentsOf: slotFile(romTag: romTag, slot: slot))
    }
}

/// A small FNV-1a 64-bit hash over the ROM's raw bytes -- a real content
/// hash (unlike `EmulatorViewModel`'s previous `bytes.count`-derived tag,
/// which only distinguished ROMs by size and would silently collide two
/// different same-size ROMs into the same save-slot directory). Any stable,
/// deterministic, collision-safe-enough hash works here (this bridge's
/// `MobileEmulator.loadState` independently rejects a tag mismatch on
/// restore regardless); FNV-1a is simple, dependency-free, and matches the
/// spirit of Android's CRC32-based `crc32Tag` in `MainActivity.kt`.
func fnv1aRomTag(_ bytes: Data) -> UInt64 {
    var hash: UInt64 = 0xCBF2_9CE4_8422_2325 // FNV offset basis
    let prime: UInt64 = 0x0000_0100_0000_01B3 // FNV prime
    for byte in bytes {
        hash ^= UInt64(byte)
        hash = hash &* prime
    }
    return hash
}
