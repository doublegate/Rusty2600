//! TIA audio — the VCS's two independent sound channels.
//!
//! Audio is part of the TIA silicon (NOT the RIOT). Each of the two channels
//! has three write registers:
//!
//! - `AUDCx` (4-bit) — the waveform / poly-counter control selecting one of 16
//!   tone modes (pure divide-by-2, 4/5/9-bit polynomial-counter noise, etc.).
//! - `AUDFx` (5-bit) — the frequency divider (the channel's clock is the
//!   horizontal-sync rate / (`AUDFx` + 1)).
//! - `AUDVx` (4-bit) — the linear volume.
//!
//! A faithful model is a pair of linear-feedback shift registers (the poly
//! counters) gated by the divider, summed and scaled — `RustyNES`'s non-linear
//! NES mixer has no analogue here; the 2600 mix is a simple linear sum of the
//! two 4-bit volumes. Stub now; pin against audio test ROMs later.

/// One TIA audio channel's register state + poly-counter phase.
#[derive(Debug, Default, Clone)]
pub struct Channel {
    /// `AUDCx` — 4-bit waveform / poly-counter control.
    pub control: u8,
    /// `AUDFx` — 5-bit frequency divider.
    pub freq: u8,
    /// `AUDVx` — 4-bit linear volume.
    pub volume: u8,
    /// Divider countdown toward the next poly-counter clock.
    divider: u8,
    /// Current poly-counter / waveform output bit (0 or 1).
    output: u8,
    // TODO(T-PS-030): the 4-bit + 5-bit + 9-bit LFSR poly-counter state.
}

impl Channel {
    /// Advance one audio clock (the horizontal-line rate). Returns the channel's
    /// current sample contribution (`0..=15`, i.e. `output * volume`).
    // Not `const`: the stub holds output steady; the real poly-counter clocking
    // mutates LFSR state, so const-ness would have to be reverted.
    #[allow(clippy::missing_const_for_fn)]
    fn tick(&mut self) -> u8 {
        // TODO(T-PS-031): clock the divider, advance the poly counter selected
        // by `control`, and update `output`. This stub holds output steady.
        if self.divider == 0 {
            self.divider = self.freq;
        } else {
            self.divider -= 1;
        }
        self.output * self.volume
    }
}

/// The TIA's two-channel audio generator. The owning `Tia` clocks this and the
/// `rusty2600-core` `AudioBus` draws the mixed samples FROM here.
#[derive(Debug, Default, Clone)]
pub struct Audio {
    /// The two independent channels.
    pub channels: [Channel; 2],
    /// Last mixed sample (linear sum of the two channels, `0..=30`).
    last_sample: u8,
}

impl Audio {
    /// Construct silent.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance both channels one audio clock and mix. Linear sum — no non-linear
    /// lookup table (that is an NES-ism with no 2600 analogue).
    pub fn tick(&mut self) {
        let a = self.channels[0].tick();
        let b = self.channels[1].tick();
        self.last_sample = a + b;
    }

    /// The most recent mixed sample (`0..=30`), for the frontend resampler.
    #[must_use]
    pub const fn sample(&self) -> u8 {
        self.last_sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_by_default() {
        let mut audio = Audio::new();
        audio.tick();
        assert_eq!(audio.sample(), 0);
    }
}
