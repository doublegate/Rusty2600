import RustyMobileFFI
import SwiftUI
import UniformTypeIdentifiers

/// The v1.12.0 "Pocket" main screen — the iOS counterpart to
/// `MainActivity.kt`: loads a ROM via `.fileImporter`, renders
/// `EmulatorViewModel.rgba` through `EmulatorView` (Metal), and wires
/// on-screen Up/Down/Left/Right/Fire/Select/Reset controls plus the
/// `PaddleControlView`. The emulator instance, input snapshot, and run loop
/// itself live in `EmulatorViewModel` (see its doc comment for why).
///
/// `v2.11.0` "Field Trip" adds the Save State / Load State slot picker
/// (`SaveStateSlotPickerView`, backed by `SaveSlots`) on top of that — no
/// HD-pack loading yet; same scope as the Android build otherwise.
struct ContentView: View {
    @StateObject private var vm = EmulatorViewModel()
    @State private var showFileImporter = false
    @State private var showSaveStatePicker = false
    @State private var showLoadStatePicker = false

    var body: some View {
        VStack(spacing: 16) {
            EmulatorView(rgba: $vm.rgba)
                .aspectRatio(CGFloat(frameWidth) / CGFloat(frameHeight), contentMode: .fit)
                .background(Color.black)

            HStack(spacing: 24) {
                dPad
                PaddleControlView(
                    position: Binding(get: { vm.paddle0Position }, set: { vm.paddle0Position = $0 }),
                    onFireChanged: { vm.paddle0Fire = $0 }
                )
                fireAndSystemButtons
            }
            .padding()

            HStack(spacing: 12) {
                Button("Load ROM") { showFileImporter = true }
                    .buttonStyle(.borderedProminent)

                Button("Save State") {
                    vm.refreshSaveSlots()
                    showSaveStatePicker = true
                }
                .buttonStyle(.bordered)
                .disabled(vm.currentRomTag == nil)

                Button("Load State") {
                    vm.refreshSaveSlots()
                    showLoadStatePicker = true
                }
                .buttonStyle(.bordered)
                .disabled(vm.currentRomTag == nil)
            }

            if let loadError = vm.loadError {
                Text(loadError).foregroundColor(.red).font(.footnote)
            }
        }
        .fileImporter(isPresented: $showFileImporter, allowedContentTypes: [.data]) { result in
            switch result {
            case .success(let url):
                vm.loadRom(from: url)
            case .failure(let error):
                vm.loadError = error.localizedDescription
            }
        }
        .sheet(isPresented: $showSaveStatePicker) {
            SaveStateSlotPickerView(mode: .save, slots: vm.saveSlots) { slot in
                vm.saveState(slot: slot)
            }
        }
        .sheet(isPresented: $showLoadStatePicker) {
            SaveStateSlotPickerView(mode: .load, slots: vm.saveSlots) { slot in
                vm.loadState(slot: slot)
            }
        }
        .onDisappear { vm.stop() }
    }

    private var dPad: some View {
        VStack(spacing: 4) {
            holdButton("▲") { vm.joystick0.up = $0 }
            HStack(spacing: 4) {
                holdButton("◀") { vm.joystick0.left = $0 }
                holdButton("▶") { vm.joystick0.right = $0 }
            }
            holdButton("▼") { vm.joystick0.down = $0 }
        }
    }

    private var fireAndSystemButtons: some View {
        VStack(spacing: 8) {
            holdButton("FIRE") { vm.joystick0.fire = $0 }
            holdButton("SELECT") { vm.switches.select = $0 }
            holdButton("RESET") { vm.switches.reset = $0 }
        }
    }

    private func holdButton(_ label: String, set: @escaping (Bool) -> Void) -> some View {
        Text(label)
            .frame(width: 56, height: 40)
            .background(Color.gray.opacity(0.3))
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { _ in set(true) }
                    .onEnded { _ in set(false) }
            )
    }
}
