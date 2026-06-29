//! Bankswitch-tier honesty gate (ADR 0003).
//!
//! The project's headline accuracy claim ("N bankswitch schemes, `AccuracyCoin`
//! 100%") is only honest if no `BestEffort` board — register-decode-only,
//! deliberately NOT covered by the accuracy / commercial-ROM oracle — ever
//! silently backs an accuracy-oracle ROM. This test enforces that invariant:
//! every board the accuracy battery covers must report a `Core` or `Curated`
//! (accuracy-gated) tier, never `BestEffort`.
//!
//! The asserted oracle set is intentionally small for now — only the `Core`-tier
//! sized boards (`2K` / `4K` / `F8`) exist as stubs. As `Curated` boards
//! (`F6`/`F4`/`E0`/`E7`/`FE`/`3F`/Superchip/`DPC`...) and `BestEffort` boards
//! land, EXTEND the oracle set below to include every new scheme the accuracy
//! battery is wired to exercise, and the gate keeps the pass-rate truthful.

use rusty2600_core::Tier;
use rusty2600_core::cart::{BankF8, Board, Rom2K, Rom4K};

/// Build each board the accuracy battery currently covers, paired with a label
/// for the failure message. These are exactly the boards whose ROMs may appear
/// in the byte-identity oracle corpus. EXTEND this as schemes land.
fn oracle_boards() -> Vec<(&'static str, Box<dyn Board>)> {
    vec![
        (
            "Rom2K",
            Box::new(Rom2K::new(&[0u8; 0x0800]).unwrap()) as Box<dyn Board>,
        ),
        (
            "Rom4K",
            Box::new(Rom4K::new(&[0u8; 0x1000]).unwrap()) as Box<dyn Board>,
        ),
        (
            "BankF8",
            Box::new(BankF8::new(&[0u8; 0x2000]).unwrap()) as Box<dyn Board>,
        ),
    ]
}

/// Headless gate: every board in the accuracy-oracle set is accuracy-gated
/// (Core or Curated). A `BestEffort` board here would mean the pass-rate is
/// silently propped up by an unverified scheme — exactly what ADR 0003 forbids.
#[test]
fn no_besteffort_board_backs_the_oracle() {
    let boards = oracle_boards();
    assert!(
        !boards.is_empty(),
        "the accuracy-oracle board set is empty — nothing to gate"
    );
    for (label, board) in &boards {
        let tier = board.tier();
        assert!(
            tier.is_accuracy_gated(),
            "accuracy-oracle board {label} reports tier {} — an oracle must be backed by a \
             Core/Curated board, never BestEffort (ADR 0003 honesty invariant)",
            tier.name(),
        );
    }
}

/// The tier predicate itself is the load-bearing structural guarantee: only
/// `BestEffort` is non-accuracy-gated, so a board can never be both that tier
/// and in the oracle set above without this gate failing.
#[test]
fn besteffort_is_structurally_never_accuracy_gated() {
    assert!(Tier::Core.is_accuracy_gated());
    assert!(Tier::Curated.is_accuracy_gated());
    assert!(!Tier::BestEffort.is_accuracy_gated());
}
