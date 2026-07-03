import SwiftUI

/// The `v2.11.0` "Field Trip" Save State / Load State slot picker -- the
/// SwiftUI counterpart to `android/`'s `MainActivity.showSaveStateDialog` /
/// `showLoadStateDialog` (`AlertDialog` + `ArrayAdapter`). Presented as a
/// sheet from `ContentView`; shows all `SaveSlots.slotCount` slots labeled
/// via `SaveSlots.SlotInfo.label`, greying out (disabling) empty slots in
/// `.load` mode the same way Android's `ArrayAdapter.isEnabled` override
/// does -- a slot must actually exist before it can be loaded, though
/// `MobileEmulator.loadState`'s own ROM-tag check is the authoritative
/// guard either way.
struct SaveStateSlotPickerView: View {
    /// Which action this picker performs -- shared UI, different semantics
    /// for whether an empty slot is selectable.
    enum Mode: Equatable {
        case save
        case load

        var title: String {
            switch self {
            case .save: return "Save State"
            case .load: return "Load State"
            }
        }
    }

    let mode: Mode
    let slots: [SaveSlots.SlotInfo]
    let onSelect: (Int) -> Void

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List(slots, id: \.slot) { slot in
                Button {
                    onSelect(slot.slot)
                    dismiss()
                } label: {
                    Text(slot.label)
                }
                .disabled(mode == .load && !slot.exists)
            }
            .navigationTitle(mode.title)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
    }
}
