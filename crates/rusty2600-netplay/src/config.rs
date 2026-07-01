//! The [`ggrs::Config`] binding for Rusty2600's rollback sessions, and
//! [`PortInput`] — the per-player wire type GGRS actually needs.
//!
//! **Why not `rusty2600_core::MovieFrame` directly as `Config::Input`** (the
//! first, tempting design): `MovieFrame` packs BOTH joystick ports' bits
//! into one `swcha` byte (bits 7-4 = port 0, bits 3-0 = port 1) plus shared
//! console-switch state, because a `.r26m` movie records the WHOLE machine's
//! input for one frame. But GGRS's `Config::Input` is fundamentally
//! per-player: each peer contributes, transmits, and confirms only ITS OWN
//! input, then GGRS hands the session both players' confirmed/predicted
//! inputs back together on `AdvanceFrame`. Using `MovieFrame` as-is would
//! mean each player's "input" secretly smuggled the OTHER player's port bits
//! too, which is both semantically wrong and a real bug waiting to happen
//! (whichever side's `MovieFrame` "wins" would silently overwrite the other
//! port). [`PortInput`] is the correct, minimal per-player type instead: one
//! joystick's four directions + fire. [`crate::frame_advance::combine`]
//! recombines both players' confirmed `PortInput`s into a real `MovieFrame`
//! immediately before advancing the frame.
//!
//! **Deliberately out of scope this release**: console switches
//! (Select/Reset/Color/Difficulty) and paddles aren't modeled per-player
//! here — there's no natural "which peer owns this" mapping for shared
//! machine-level switches in a 2-player session, and paddle-based head-to-head
//! titles are a small slice of the 2600 library. Both are documented,
//! scoped follow-ups, not silently dropped: a session runs with console
//! switches permanently idle and paddles centered until this is revisited.

use ggrs::Config;

/// One player's joystick contribution for one frame.
///
/// Four directions (active HIGH — `true` = pressed, unlike
/// `MovieFrame::swcha`'s active-low RIOT convention, since this is a wire
/// type with no hardware register to mirror) plus the single fire button.
/// Five independent binary switches map directly onto real joystick
/// hardware — a state-machine/enum refactor would be premature abstraction
/// for exactly what a 2600 joystick physically has.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct PortInput {
    /// Up.
    pub up: bool,
    /// Down.
    pub down: bool,
    /// Left.
    pub left: bool,
    /// Right.
    pub right: bool,
    /// The single fire button.
    pub fire: bool,
}

/// Compile-time parameterization for every Rusty2600 rollback session.
#[derive(Debug)]
pub struct RustyConfig;

impl Config for RustyConfig {
    type Input = PortInput;
    type InputPredictor = ggrs::PredictRepeatLast;
    // A `rusty2600_core::SaveState::encode()` blob. GGRS only requires
    // `Clone` (default feature set) of the state type; it never inspects it,
    // just hands it back on `LoadGameState`.
    type State = Vec<u8>;
    // GGRS's own bound (`Clone + PartialEq + Eq + Hash + Debug`, or
    // `+ Send + Sync` under the `sync-send` feature this crate doesn't
    // enable) is exactly what `SocketAddr` already implements.
    type Address = std::net::SocketAddr;
}
