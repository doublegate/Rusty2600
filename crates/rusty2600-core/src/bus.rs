//! The Bus owns everything mutable.
//!
//! That is the TIA (video + audio), the RIOT (RAM + I/O + timer), the cart
//! (→ bankswitch board / coprocessor), the controllers, and the open-bus latch.
//! The CPU borrows `&mut Bus` during `tick`.
//!
//! The 2600 has **no separate work RAM** — the only general RAM is the RIOT's
//! 128 bytes, so the Bus does NOT carry a WRAM field (unlike the NES Bus). The
//! TIA's audio is drawn through [`AudioBus`] (audio lives in the TIA, per the
//! chip layout). See `docs/architecture.md` (the load-bearing facts).

use alloc::boxed::Box;

use rusty2600_cart::Board;
use rusty2600_riot::Riot;
use rusty2600_tia::Tia;

/// Everything mutable lives here.
///
/// The TetaNES-postmortem lesson — one owner avoids the borrow-checker fight.
/// The CPU borrows this; the TIA and RIOT see narrower trait views
/// ([`VideoBus`] / [`AudioBus`]).
#[derive(Default)]
pub struct Bus {
    /// The TIA — video beam-racing + the two audio channels.
    pub tia: Tia,
    /// The RIOT — the console's only RAM, the I/O ports, and the interval timer.
    pub riot: Riot,
    /// The cartridge bankswitch board (boxed: the scheme is chosen at load
    /// time). `None` until a ROM is loaded.
    pub board: Option<Box<dyn Board>>,
    /// Open-bus latch — the last value driven on the data bus, returned for
    /// reads of unmapped / write-only addresses.
    pub open_bus: u8,
    // TODO(T-PS-050): controller / paddle / console-switch input state.
}

impl core::fmt::Debug for Bus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Bus")
            .field("tia", &self.tia)
            .field("riot", &self.riot)
            .field("board", &self.board.as_ref().map(|_| "<dyn Board>"))
            .field("open_bus", &self.open_bus)
            .finish()
    }
}

impl Bus {
    /// Construct an empty bus (no ROM loaded).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Decode a CPU read across the 13-bit (8 KiB) 6507 address window.
    ///
    /// The VCS address map is sparse and heavily mirrored: `$00..=$7F`
    /// (with mirrors) hits the TIA, `$80..=$FF` the RIOT RAM, `$280..=$29F` the
    /// RIOT I/O + timer, and `$1000..=$1FFF` the cartridge. This stub returns
    /// the open-bus latch; the real decode is a `// TODO`.
    // Not `const`: the real decode mutates the open-bus latch and runs cart
    // hotspot bank logic, so const-ness would have to be reverted.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = addr & 0x1FFF; // 6507 drives only A0..=A12.
        // TODO(T-PS-051): full sparse/mirrored decode (TIA read regs, RIOT RAM,
        // RIOT I/O+timer, cart window). Cart reads must run hotspot bank logic.
        let _ = addr;
        self.open_bus
    }

    /// Decode a CPU write across the 13-bit address window. Updates the open-bus
    /// latch with the written value.
    // Not `const`: the real decode routes into the TIA/RIOT/cart, so const-ness
    // would have to be reverted.
    #[allow(clippy::missing_const_for_fn)]
    pub fn cpu_write(&mut self, addr: u16, val: u8) {
        let addr = addr & 0x1FFF;
        self.open_bus = val;
        // TODO(T-PS-052): route to TIA write regs, RIOT RAM/I/O/timer, or the
        // cart window (write-triggered hotspots).
        let _ = addr;
    }
}

/// The narrow trait the video path sees. Beam-raced output has no framebuffer,
/// so this exposes the per-dot emit + the cart-mediated reads the TIA needs.
pub trait VideoBus {
    /// Read a byte the TIA needs from the cartridge/board side of the bus.
    fn video_read(&mut self, addr: u16) -> u8;
}

/// The narrow trait the audio path sees. The core draws samples FROM the TIA
/// (audio lives in the TIA), so this is the read-side handle the frontend
/// resampler pulls through.
pub trait AudioBus {
    /// The TIA's most recent mixed audio sample.
    fn audio_sample(&self) -> u8;
}

impl AudioBus for Bus {
    fn audio_sample(&self) -> u8 {
        self.tia.audio.sample()
    }
}
