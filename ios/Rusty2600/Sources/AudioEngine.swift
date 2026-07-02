import AVFoundation

/// Plays `FrameOutput.audioSamples` (already DC-blocked, normalized `Float`
/// samples at the TIA's native ~31.4kHz rate) through `AVAudioEngine` — the
/// iOS counterpart to Android's `AudioTrack` in `ENCODING_PCM_FLOAT` mode.
///
/// `AVAudioPlayerNode.scheduleBuffer` already queues buffers gaplessly on
/// its own internal timeline, so — unlike the GH Pages wasm build's
/// `AudioSink` (which has to track `next_start` itself against the raw Web
/// Audio API) — no manual scheduling/timing bookkeeping is needed here.
final class AudioEngine {
    private let engine = AVAudioEngine()
    private let player = AVAudioPlayerNode()
    private let format: AVAudioFormat

    /// `3_579_545 / 114` color clocks per sample — the same rate
    /// `MainActivity.buildAudioTrack()` computes on the Android side.
    init() {
        let sampleRate = 3_579_545.0 / 114.0
        format = AVAudioFormat(
            commonFormat: .pcmFormatFloat32,
            sampleRate: sampleRate,
            channels: 1,
            interleaved: false
        )!

        engine.attach(player)
        engine.connect(player, to: engine.mainMixerNode, format: format)

        do {
            try AVAudioSession.sharedInstance().setCategory(.playback, mode: .default)
            try AVAudioSession.sharedInstance().setActive(true)
            try engine.start()
            player.play()
        } catch {
            // A running audio session isn't guaranteed on every host state
            // (e.g. an interrupted session); the emulator itself still runs
            // and produces frames with or without audio output.
        }
    }

    /// Enqueue one frame's worth of samples. Safe to call from a background
    /// thread — `AVAudioPlayerNode` accepts scheduling calls off the main
    /// thread.
    func push(_ samples: [Float]) {
        guard !samples.isEmpty else { return }
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(samples.count)) else {
            return
        }
        buffer.frameLength = AVAudioFrameCount(samples.count)
        samples.withUnsafeBufferPointer { src in
            buffer.floatChannelData?[0].update(from: src.baseAddress!, count: samples.count)
        }
        player.scheduleBuffer(buffer, completionHandler: nil)
    }
}
