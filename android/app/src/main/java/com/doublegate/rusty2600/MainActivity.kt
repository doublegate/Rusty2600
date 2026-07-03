package com.doublegate.rusty2600

import android.app.AlertDialog
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.view.MotionEvent
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.Toast
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import java.util.zip.CRC32
import kotlin.math.roundToInt
import uniffi.rusty2600_mobile.MobileEmulator
import uniffi.rusty2600_mobile.MobileException
import uniffi.rusty2600_mobile.MobileInput
import uniffi.rusty2600_mobile.MobileJoystick
import uniffi.rusty2600_mobile.MobilePaddle
import uniffi.rusty2600_mobile.MobileSwitches

/**
 * The v1.11.0 "Handheld" verification host: loads a ROM via the system file
 * picker, drives `MobileEmulator.runFrame` at ~60Hz on a background thread,
 * blits the returned RGBA framebuffer to [EmulatorView], and plays the
 * returned float-PCM audio through an [AudioTrack] in
 * `ENCODING_PCM_FLOAT` mode — no resampling needed on either side, since
 * `AudioTrack` accepts the TIA's native ~31.4kHz rate directly.
 *
 * `v2.11.0` "Field Trip" adds the Save State / Load State slot picker (see
 * [SaveSlots]) on top of that verification build — no paddle input, no
 * HD-pack loading yet; see `docs/mobile.md` for what's in scope this
 * release.
 */
class MainActivity : AppCompatActivity() {

    private val emulator = MobileEmulator()
    private lateinit var emulatorView: EmulatorView
    private lateinit var audioTrack: AudioTrack

    /** The currently-loaded ROM's identity tag (see `crc32Tag`), or `null` before any ROM is loaded. Keys [SaveSlots]. */
    private var currentRomTag: ULong? = null

    private val input =
        MobileInput(
            joystick0 = MobileJoystick(up = false, down = false, left = false, right = false, fire = false),
            joystick1 = MobileJoystick(up = false, down = false, left = false, right = false, fire = false),
            paddle0 = MobilePaddle(position = 0u, fire = false),
            paddle1 = MobilePaddle(position = 0u, fire = false),
            paddle2 = MobilePaddle(position = 0u, fire = false),
            paddle3 = MobilePaddle(position = 0u, fire = false),
            switches =
                MobileSwitches(
                    select = false,
                    reset = false,
                    color = true,
                    leftDifficulty = false,
                    rightDifficulty = false,
                ),
        )

    private val emuThread = HandlerThread("rusty2600-emu").apply { start() }
    private lateinit var emuHandler: Handler

    @Volatile private var running = false

    private val openRom =
        registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
            uri?.let(::loadRom)
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        emulatorView = findViewById(R.id.emulatorView)
        emuHandler = Handler(emuThread.looper)
        audioTrack = buildAudioTrack().apply { play() }

        findViewById<Button>(R.id.loadRomButton).setOnClickListener { openRom.launch(arrayOf("*/*")) }
        findViewById<Button>(R.id.saveStateButton).setOnClickListener { showSaveStateDialog() }
        findViewById<Button>(R.id.loadStateButton).setOnClickListener { showLoadStateDialog() }
        bindHold(R.id.upButton) { input.joystick0.up = it }
        bindHold(R.id.downButton) { input.joystick0.down = it }
        bindHold(R.id.leftButton) { input.joystick0.left = it }
        bindHold(R.id.rightButton) { input.joystick0.right = it }
        bindHold(R.id.fireButton) { input.joystick0.fire = it }
        bindHold(R.id.resetButton) { input.switches.reset = it }
        bindHold(R.id.selectButton) { input.switches.select = it }
    }

    /** TIA's native audio rate: `3_579_545 / 114` color clocks per sample. */
    private fun buildAudioTrack(): AudioTrack {
        val sampleRate = (3_579_545.0 / 114.0).roundToInt()
        val format =
            AudioFormat.Builder()
                .setEncoding(AudioFormat.ENCODING_PCM_FLOAT)
                .setSampleRate(sampleRate)
                .setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
                .build()
        val minBuf = AudioTrack.getMinBufferSize(sampleRate, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_FLOAT)
        return AudioTrack(
            AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_GAME)
                .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                .build(),
            format,
            minBuf * 4,
            AudioTrack.MODE_STREAM,
            AudioManager.AUDIO_SESSION_ID_GENERATE,
        )
    }

    private fun bindHold(id: Int, set: (Boolean) -> Unit) {
        findViewById<Button>(id).setOnTouchListener { _, event ->
            when (event.action) {
                MotionEvent.ACTION_DOWN -> set(true)
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> set(false)
            }
            false
        }
    }

    private fun loadRom(uri: Uri) {
        val bytes = contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: return
        val romTag = crc32Tag(bytes)
        try {
            emulator.loadRom(bytes, romTag)
            currentRomTag = romTag
            startLoop()
        } catch (e: MobileException) {
            Toast.makeText(this, "Load failed: ${e.message}", Toast.LENGTH_LONG).show()
        }
    }

    /**
     * The `v2.11.0` "Field Trip" Save State picker: 8 numbered slots (matching
     * desktop's [`rusty2600_frontend::config::SAVE_SLOT_COUNT`] convention),
     * each labeled with its occupied/empty status and timestamp
     * ([SaveSlots.label]). Tapping any slot captures the running emulator's
     * current state (`MobileEmulator.saveState`) and writes it into that
     * slot ([SaveSlots.save]), overwriting whatever was there before.
     */
    private fun showSaveStateDialog() {
        val romTag = currentRomTag
        if (romTag == null) {
            Toast.makeText(this, R.string.no_rom_loaded, Toast.LENGTH_SHORT).show()
            return
        }
        val slots = SaveSlots.probeAll(this, romTag)
        AlertDialog.Builder(this)
            .setTitle(R.string.dialog_save_state_title)
            .setItems(slots.map { it.label() }.toTypedArray()) { _, index ->
                try {
                    val bytes = emulator.saveState()
                    SaveSlots.save(this, romTag, index, bytes)
                    Toast.makeText(this, getString(R.string.saved_to_slot, index), Toast.LENGTH_SHORT).show()
                } catch (e: MobileException) {
                    Toast.makeText(this, getString(R.string.save_failed, e.message), Toast.LENGTH_LONG).show()
                }
            }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    /**
     * The Load State counterpart to [showSaveStateDialog]: empty slots are
     * shown but disabled (greyed out, not clickable) via the custom
     * [ArrayAdapter.isEnabled] override below, matching desktop's
     * `add_enabled(slot.exists, ...)` menu-item gating. Restoring a slot
     * whose embedded ROM tag doesn't match the currently-loaded ROM is
     * additionally rejected by `MobileEmulator.loadState` itself (the same
     * bridge-level check desktop's `SaveState::restore` performs) -- this
     * dialog's own per-ROM slot directory keying ([SaveSlots]) means that
     * mismatch can't actually happen in practice, but the bridge check stays
     * as the authoritative guard either way.
     */
    private fun showLoadStateDialog() {
        val romTag = currentRomTag
        if (romTag == null) {
            Toast.makeText(this, R.string.no_rom_loaded, Toast.LENGTH_SHORT).show()
            return
        }
        val slots = SaveSlots.probeAll(this, romTag)
        val adapter =
            object : ArrayAdapter<String>(this, android.R.layout.simple_list_item_1, slots.map { it.label() }) {
                override fun isEnabled(position: Int) = slots[position].exists
            }
        AlertDialog.Builder(this)
            .setTitle(R.string.dialog_load_state_title)
            .setAdapter(adapter) { _, index ->
                val bytes = SaveSlots.load(this, romTag, index) ?: return@setAdapter
                try {
                    emulator.loadState(bytes)
                    Toast.makeText(this, getString(R.string.loaded_slot, index), Toast.LENGTH_SHORT).show()
                } catch (e: MobileException) {
                    Toast.makeText(this, getString(R.string.load_failed, e.message), Toast.LENGTH_LONG).show()
                }
            }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    private fun startLoop() {
        if (running) return
        running = true
        emuHandler.post(
            object : Runnable {
                override fun run() {
                    if (!running) return
                    val out = emulator.runFrame(input)
                    runOnUiThread { emulatorView.updateFrame(out.rgba) }
                    val samples = out.audioSamples
                    if (samples.isNotEmpty()) {
                        val pcm = samples.toFloatArray()
                        audioTrack.write(pcm, 0, pcm.size, AudioTrack.WRITE_BLOCKING)
                    }
                    emuHandler.postDelayed(this, FRAME_INTERVAL_MS)
                }
            },
        )
    }

    override fun onDestroy() {
        running = false
        audioTrack.release()
        emuThread.quitSafely()
        emulator.destroy()
        super.onDestroy()
    }

    /** An opaque, host-supplied ROM identity; CRC32 is enough to key save-states. */
    private fun crc32Tag(bytes: ByteArray): ULong = CRC32().apply { update(bytes) }.value.toULong()

    private companion object {
        const val FRAME_INTERVAL_MS = 16L
    }
}
