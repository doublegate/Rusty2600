package com.doublegate.rusty2600

import android.content.Context
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone

/**
 * The `v2.11.0` "Field Trip" on-device save-state slot store — mirrors
 * `rusty2600-frontend`'s own manual save-state slot convention
 * (`crates/rusty2600-frontend/src/config.rs`, `v2.4.0` "Save Point")
 * adapted to Android's app-private storage: [SLOT_COUNT] numbered slots
 * (`0..7`), one file per slot named `slot_<N>.<EXTENSION>`, all of one ROM's
 * slots keyed under a directory named after that ROM's identity tag so two
 * different ROMs' slots can never collide or be silently cross-loaded.
 *
 * The ROM tag here is the same CRC32 `romTag` [MainActivity] already
 * computes (`crc32Tag`) to pass to `MobileEmulator.loadRom` and which
 * `MobileEmulator.loadState` validates a restored blob against — reusing it
 * for the slot directory name means a slot can never even be *written*
 * under the wrong ROM's key, on top of the bridge's own tag check on load.
 *
 * Uses `Context.filesDir` (app-private internal storage), matching desktop's
 * own intent of a private, per-app save-data directory
 * (`directories::ProjectDirs::data_dir()`) rather than shared/external
 * storage that other apps or the user could tamper with.
 */
object SaveSlots {
    /** Matches `rusty2600_frontend::config::SAVE_SLOT_COUNT`. */
    const val SLOT_COUNT = 8

    /** Matches `rusty2600_frontend::config`'s `.r26s` slot-file extension. */
    private const val EXTENSION = "r26s"

    /** One save-state slot's on-disk status, probed fresh from the filesystem. */
    data class SlotInfo(val slot: Int, val exists: Boolean, val modifiedMillis: Long?)

    // `ULong.toString(16)` + `padStart` (not a `Long` cast through `"%016x".format`, PR #21 bot
    // review, Gemini Code Assist): the cast relied on JVM-specific `%x` behavior for a value with
    // the sign bit set once reinterpreted as `Long`, which happens to work but isn't guaranteed
    // idiomatic Kotlin; `ULong`'s own `toString(radix)` has no such ambiguity.
    private fun slotDir(context: Context, romTag: ULong): File =
        File(context.filesDir, "saves/${romTag.toString(16).padStart(16, '0')}")

    /** The path to one save-state slot file (`<slot-dir>/slot_<N>.r26s`). */
    fun slotFile(context: Context, romTag: ULong, slot: Int): File =
        File(slotDir(context, romTag), "slot_$slot.$EXTENSION")

    /** Probes [slot]'s current on-disk status for `romTag`. */
    fun probe(context: Context, romTag: ULong, slot: Int): SlotInfo {
        val file = slotFile(context, romTag, slot)
        return if (file.exists()) {
            SlotInfo(slot, exists = true, modifiedMillis = file.lastModified())
        } else {
            SlotInfo(slot, exists = false, modifiedMillis = null)
        }
    }

    /** Probes all [SLOT_COUNT] slots for `romTag`, in slot order. */
    fun probeAll(context: Context, romTag: ULong): List<SlotInfo> = (0 until SLOT_COUNT).map { probe(context, romTag, it) }

    /** Writes `bytes` (a `MobileEmulator.saveState()` blob) into `slot`, creating the ROM's slot directory if needed. */
    fun save(context: Context, romTag: ULong, slot: Int, bytes: ByteArray) {
        val file = slotFile(context, romTag, slot)
        file.parentFile?.mkdirs()
        file.writeBytes(bytes)
    }

    /** Reads `slot`'s blob back, or `null` if the slot is empty. Pass the result straight to `MobileEmulator.loadState`. */
    fun load(context: Context, romTag: ULong, slot: Int): ByteArray? {
        val file = slotFile(context, romTag, slot)
        return if (file.exists()) file.readBytes() else null
    }
}

/**
 * A human-readable menu label, e.g. `"Slot 3 (empty)"` or a slot with a
 * real save shown as `"Slot 3 -- 2026-07-02 14:03:07 UTC"` -- the exact
 * wording `rusty2600_frontend::shell::SaveSlotInfo::label` uses on desktop,
 * so the mobile UX reads as the same feature. A top-level extension
 * (rather than a member of [SaveSlots]) so it's visible from [MainActivity]
 * with no import needed (same-package top-level declarations are always
 * in scope in Kotlin).
 */
fun SaveSlots.SlotInfo.label(): String {
    val millis = modifiedMillis ?: return "Slot $slot (empty)"
    val fmt = SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US)
    fmt.timeZone = TimeZone.getTimeZone("UTC")
    return "Slot $slot -- ${fmt.format(Date(millis))} UTC"
}
