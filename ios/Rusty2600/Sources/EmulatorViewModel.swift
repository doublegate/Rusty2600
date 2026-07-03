import Foundation
import RustyMobileFFI

/// Owns the `MobileEmulator` instance, the input snapshot, and the ~60Hz run
/// loop — pulled out of `ContentView` into an `ObservableObject` rather than
/// raw `@State`, since SwiftUI view structs can be recreated at any time and
/// a long-lived background loop needs stable identity to mutate into, the
/// same reason Android's `MainActivity` keeps this state in the Activity
/// (not a transient Compose/View-local scope either).
@MainActor
final class EmulatorViewModel: ObservableObject {
    private let emulator = MobileEmulator()
    private let audio = AudioEngine()
    private var loopTask: Task<Void, Never>?

    @Published var rgba = [UInt8](repeating: 0, count: frameWidth * frameHeight * 4)
    @Published var loadError: String?
    @Published var isRunning = false
    @Published var saveSlots: [SaveSlots.SlotInfo] = []

    var joystick0 = MobileJoystick(up: false, down: false, left: false, right: false, fire: false)
    var switches = MobileSwitches(
        select: false, reset: false, color: true, leftDifficulty: false, rightDifficulty: false
    )
    var paddle0Position: UInt8 = 128
    var paddle0Fire = false

    /// The currently-loaded ROM's identity tag (see `fnv1aRomTag`), or `nil`
    /// before any ROM is loaded. Keys [SaveSlots] the same way Android's
    /// `MainActivity.currentRomTag` keys `SaveSlots.kt`. `@Published` so
    /// `ContentView`'s Save State / Load State buttons re-enable themselves
    /// immediately on load, not just on the next incidental redraw.
    @Published private(set) var currentRomTag: UInt64?

    func loadRom(from url: URL) {
        guard url.startAccessingSecurityScopedResource() else {
            loadError = "Could not access the selected file"
            return
        }
        defer { url.stopAccessingSecurityScopedResource() }

        do {
            let bytes = try Data(contentsOf: url)
            let romTag = fnv1aRomTag(bytes)
            try emulator.loadRom(bytes: bytes, romTag: romTag)
            currentRomTag = romTag
            loadError = nil
            refreshSaveSlots()
            startLoopIfNeeded()
        } catch let error as MobileError {
            loadError = "Load failed: \(error)"
        } catch {
            loadError = "Load failed: \(error.localizedDescription)"
        }
    }

    /// Refreshes [saveSlots] from the filesystem for the currently-loaded
    /// ROM (a no-op, clearing the list, when no ROM is loaded).
    func refreshSaveSlots() {
        guard let currentRomTag else {
            saveSlots = []
            return
        }
        saveSlots = SaveSlots.probeAll(romTag: currentRomTag)
    }

    /// Captures the running emulator's current state
    /// (`MobileEmulator.saveState`) and writes it into `slot`
    /// ([SaveSlots.save]), overwriting whatever was there before. Mirrors
    /// `MainActivity.showSaveStateDialog`'s Android behavior exactly.
    func saveState(slot: Int) {
        guard let currentRomTag else {
            loadError = "No ROM loaded"
            return
        }
        do {
            let bytes = try emulator.saveState()
            try SaveSlots.save(romTag: currentRomTag, slot: slot, data: bytes)
            loadError = nil
            refreshSaveSlots()
        } catch let error as MobileError {
            loadError = "Save failed: \(error)"
        } catch {
            loadError = "Save failed: \(error.localizedDescription)"
        }
    }

    /// The Load State counterpart to [saveState]: a no-op (no error surfaced)
    /// when `slot` is empty, matching Android's `showLoadStateDialog`
    /// disabled-empty-slot behavior -- the caller's slot-picker UI is
    /// expected to have already excluded empty slots from selection.
    func loadState(slot: Int) {
        guard let currentRomTag else {
            loadError = "No ROM loaded"
            return
        }
        guard let bytes = SaveSlots.load(romTag: currentRomTag, slot: slot) else { return }
        do {
            try emulator.loadState(bytes: bytes)
            loadError = nil
        } catch let error as MobileError {
            loadError = "Load failed: \(error)"
        } catch {
            loadError = "Load failed: \(error.localizedDescription)"
        }
    }

    private func startLoopIfNeeded() {
        guard loopTask == nil else { return }
        isRunning = true
        loopTask = Task { [weak self] in
            while let self, self.isRunning {
                let input = MobileInput(
                    joystick0: self.joystick0,
                    joystick1: MobileJoystick(up: false, down: false, left: false, right: false, fire: false),
                    paddle0: MobilePaddle(position: self.paddle0Position, fire: self.paddle0Fire),
                    paddle1: MobilePaddle(position: 0, fire: false),
                    paddle2: MobilePaddle(position: 0, fire: false),
                    paddle3: MobilePaddle(position: 0, fire: false),
                    switches: self.switches
                )
                if let out = try? self.emulator.runFrame(input: input) {
                    self.rgba = [UInt8](out.rgba)
                    self.audio.push(out.audioSamples)
                }
                try? await Task.sleep(nanoseconds: 16_666_667) // ~60Hz
            }
        }
    }

    func stop() {
        isRunning = false
        loopTask?.cancel()
        loopTask = nil
    }
}
