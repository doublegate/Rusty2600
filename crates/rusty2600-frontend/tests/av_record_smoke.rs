//! A/V recording end-to-end smoke test (`[v2.13.0]`).
//!
//! `#[ignore]`d by default — CI runners aren't guaranteed to have `ffmpeg`
//! on `PATH`, and `av_record.rs`'s own unit tests already cover the pure
//! `ffmpeg_args`/`Container` logic without spawning anything. Run manually
//! (with `ffmpeg` installed) via:
//!
//! ```sh
//! cargo test -p rusty2600-frontend --features av-record --test av_record_smoke -- --ignored
//! ```
#![cfg(feature = "av-record")]

use rusty2600_frontend::av_record::{AvParams, AvRecorder};

#[test]
#[ignore = "requires ffmpeg on PATH; run manually with --ignored"]
fn records_a_few_frames_to_a_real_mp4() {
    let dir = std::env::temp_dir().join("rusty2600-av-record-smoke");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let out_path = dir.join("smoke.mp4");
    let _ = std::fs::remove_file(&out_path);

    let width = 160;
    let height = 192; // NTSC active height
    let params = AvParams {
        out_path: out_path.clone(),
        width,
        height,
        sample_rate: 48_000,
        fps_num: 60_098_814,
        fps_den: 1_000_000,
    };

    let mut recorder = AvRecorder::start(params).expect("ffmpeg must be on PATH for this test");

    let frame = vec![0x40u8; (width * height * 4) as usize];
    let samples = vec![0.0f32; 800]; // ~1 frame's worth at 48kHz/60fps
    for _ in 0..10 {
        recorder.push_video(&frame).expect("push_video");
        recorder.push_audio(&samples).expect("push_audio");
    }
    assert_eq!(recorder.frames(), 10);

    let final_path = recorder.stop().expect("ffmpeg mux must succeed");
    assert_eq!(final_path, out_path);

    let meta = std::fs::metadata(&final_path).expect("output file must exist");
    assert!(meta.len() > 0, "output file must be non-empty");

    // Confirm ffmpeg itself considers the file a valid, readable container
    // (not just "some bytes exist") by asking it to probe the stream info.
    let probe = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&final_path)
        .arg("-f")
        .arg("null")
        .arg("-")
        .output()
        .expect("ffmpeg probe spawn");
    assert!(
        probe.status.success(),
        "ffmpeg could not read back the produced file: {}",
        String::from_utf8_lossy(&probe.stderr)
    );

    let _ = std::fs::remove_file(&final_path);
}
