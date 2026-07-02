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

    var joystick0 = MobileJoystick(up: false, down: false, left: false, right: false, fire: false)
    var switches = MobileSwitches(
        select: false, reset: false, color: true, leftDifficulty: false, rightDifficulty: false
    )
    var paddle0Position: UInt8 = 128
    var paddle0Fire = false

    func loadRom(from url: URL) {
        guard url.startAccessingSecurityScopedResource() else {
            loadError = "Could not access the selected file"
            return
        }
        defer { url.stopAccessingSecurityScopedResource() }

        do {
            let bytes = try Data(contentsOf: url)
            let romTag = UInt64(bytes.count) &* 2_654_435_761 // a cheap opaque tag; any stable hash works
            try emulator.loadRom(bytes: bytes, romTag: romTag)
            loadError = nil
            startLoopIfNeeded()
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
