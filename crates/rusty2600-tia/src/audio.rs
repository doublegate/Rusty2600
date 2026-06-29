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
//! two 4-bit volumes.

/// One TIA audio channel's register state + poly-counter phase.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    /// 4-bit polynomial LFSR.
    poly4: u8,
    /// 5-bit polynomial LFSR.
    poly5: u8,
    /// Prescaler tracking the 114 (or 342 for CPU/114) color clocks per audio clock.
    prescale: u16,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            control: 0,
            freq: 0,
            volume: 0,
            divider: 0,
            output: 1,
            poly4: 1, // Must be non-zero to avoid LFSR lockup
            poly5: 1, // Must be non-zero to avoid LFSR lockup
            prescale: 0,
        }
    }
}

impl Channel {
    /// Advance one TIA color clock. Returns the channel's
    /// current sample contribution (`0..=15`, i.e. `output * volume`).
    #[allow(clippy::missing_const_for_fn)]
    fn tick(&mut self) -> u8 {
        let audc = self.control & 0x0F;
        // Modes 12-15 use CPU/114 which is color/342 (3 times slower than color/114)
        let limit = if audc >= 12 { 342 } else { 114 };

        self.prescale += 1;
        if self.prescale >= limit {
            self.prescale = 0;

            if self.divider == 0 {
                self.divider = self.freq;

                // Feedback calculations
                let in5 = ((self.poly5 >> 4) ^ (self.poly5 >> 3)) & 1;
                let in4 = ((self.poly4 >> 3) ^ (self.poly4 >> 2)) & 1;

                match audc {
                    0 | 11 => {
                        // Set output to 1 (volume only / digital sample mode)
                        self.output = 1;
                    }
                    1 => {
                        // 4-bit poly
                        self.poly4 = ((self.poly4 << 1) | in4) & 0x0F;
                        self.output = self.poly4 & 1;
                    }
                    2 => {
                        // 4-bit poly clocked by div-15 (465-bit composite)
                        self.poly5 = ((self.poly5 << 1) | in5) & 0x1F;
                        if (self.poly5 & 1) == 1 {
                            self.poly4 = ((self.poly4 << 1) | in4) & 0x0F;
                        }
                        self.output = self.poly4 & 1;
                    }
                    3 => {
                        // 5-bit poly -> 4-bit poly composite
                        self.poly5 = ((self.poly5 << 1) | in5) & 0x1F;
                        // Clock poly4 normally but use poly5 output in feedback
                        let composite_in4 = in4 ^ (self.poly5 & 1);
                        self.poly4 = ((self.poly4 << 1) | composite_in4) & 0x0F;
                        self.output = self.poly4 & 1;
                    }
                    4 | 5 | 12 | 13 => {
                        // Pure tone, divide-by-2
                        self.output ^= 1;
                    }
                    6 | 10 | 14 => {
                        // Divide-by-31 pure tone
                        self.poly5 = ((self.poly5 << 1) | in5) & 0x1F;
                        self.output = self.poly5 & 1;
                    }
                    7 | 15 => {
                        // 5-bit poly -> divide-by-31
                        self.poly5 = ((self.poly5 << 1) | in5) & 0x1F;
                        if (self.poly5 & 1) == 1 {
                            self.output ^= 1;
                        }
                    }
                    8 => {
                        // 9-bit poly (white noise)
                        let in9 = ((self.poly5 >> 4) ^ (self.poly4 >> 3)) & 1;
                        self.poly5 = ((self.poly5 << 1) | in9) & 0x1F;
                        // Poly4 clocks from poly5's output
                        self.poly4 = ((self.poly4 << 1) | (self.poly5 & 1)) & 0x0F;
                        self.output = self.poly4 & 1;
                    }
                    9 => {
                        // 5-bit poly
                        self.poly5 = ((self.poly5 << 1) | in5) & 0x1F;
                        self.output = self.poly5 & 1;
                    }
                    _ => {}
                }
            } else {
                self.divider -= 1;
            }
        }
        self.output * self.volume
    }
}

/// The TIA's two-channel audio generator. The owning `Tia` clocks this and the
/// `rusty2600-core` `AudioBus` draws the mixed samples FROM here.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
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

    /// Advance both channels one color clock and mix. Linear sum — no non-linear
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
