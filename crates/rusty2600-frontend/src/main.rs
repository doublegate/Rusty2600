//! `rusty2600` — the Rusty2600 frontend binary (native).
//!
//! A thin shim over `lib.rs`, which owns the module tree. The wasm32 entry point lives at
//! `lib.rs::wasm::start` (gated `#[cfg(target_arch = "wasm32")]`); when cargo builds this bin for
//! the wasm32 target we compile an empty `main` instead — the real entry is `wasm::start`.
//!
//! The native path uses a clap 4 CLI (`cli.rs`): `rusty2600 <ROM>` loads + runs; `rusty2600` with
//! no ROM opens the menu shell; `rusty2600 help [<topic>]` + `completions <shell>` are the
//! native-only help/UX subcommands. The shell is adapted from the `RustyNES` / `RustySNES`
//! `winit + wgpu + cpal + egui` frontend — only the console-specific bits differ (the beam-raced
//! display buffer, the NTSC/PAL/SECAM palette, the joystick / paddle / console-switch input map,
//! and the 6507 / TIA / RIOT debugger panels). See `docs/frontend.md`.

// First surfaced by verifying `cargo clippy --target wasm32-unknown-unknown --no-default-features
// --features wasm-winit` (`[v2.8.0]`; this exact build/feature combination had not been
// clippy-checked before) — a plain no-op fn trivially satisfies `missing_const_for_fn`.
#[cfg(target_arch = "wasm32")]
const fn main() {}

#[cfg(not(target_arch = "wasm32"))]
use std::process::ExitCode;

#[cfg(not(target_arch = "wasm32"))]
use clap::{CommandFactory as _, Parser as _};

#[cfg(not(target_arch = "wasm32"))]
use rusty2600_frontend::app::App;
#[cfg(not(target_arch = "wasm32"))]
use rusty2600_frontend::cli::{Cli, CliCommand};
#[cfg(not(target_arch = "wasm32"))]
use rusty2600_frontend::config::Config;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            // clap prints help/version to stdout (exit 0) and errors to stderr (exit 2).
            let _ = e.print();
            return ExitCode::from(u8::try_from(e.exit_code()).unwrap_or(2));
        }
    };

    match cli.command {
        Some(CliCommand::Help { topic, interactive }) => run_help(topic.as_deref(), interactive),
        Some(CliCommand::Completions { shell }) => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "rusty2600", &mut std::io::stdout());
            ExitCode::SUCCESS
        }
        None => run_emulator(cli.rom),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn run_emulator(rom: Option<std::path::PathBuf>) -> ExitCode {
    let config = Config::load();
    let app = App::with_config(config, rom);
    match app.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rusty2600: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn run_help(topic: Option<&str>, interactive: bool) -> ExitCode {
    use rusty2600_frontend::cli::{TOPICS, topic_text};

    #[cfg(feature = "help-tui")]
    if interactive {
        if let Err(e) = rusty2600_frontend::help_tui::run() {
            eprintln!("rusty2600: help TUI error: {e}");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }
    #[cfg(not(feature = "help-tui"))]
    let _ = interactive;

    topic.map_or_else(
        || {
            println!("Rusty2600 help topics:");
            for t in TOPICS {
                println!("  {t}");
            }
            println!("\nRun `rusty2600 help <topic>` (or `--interactive` for the TUI browser).");
            ExitCode::SUCCESS
        },
        |t| {
            topic_text(t).map_or_else(
                || {
                    eprintln!(
                        "rusty2600: unknown help topic '{t}'. Known: {}",
                        TOPICS.join(", ")
                    );
                    ExitCode::FAILURE
                },
                |body| {
                    println!("{body}");
                    ExitCode::SUCCESS
                },
            )
        },
    )
}
