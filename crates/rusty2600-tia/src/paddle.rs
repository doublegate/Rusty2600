//! `INPT0..=INPT3` analog paddle input — a real RC-circuit simulation, not a
//! plain digital register.
//!
//! Real Atari 2600 paddles are variable resistors (0..=1MΩ) wired through an
//! RC circuit to the TIA's four analog comparator inputs. A game reads a
//! paddle's position by grounding the capacitor via a `VBLANK` write (the
//! "dump"), releasing the dump, and counting how many scanlines/cycles pass
//! before the corresponding `INPTx` bit flips high once the capacitor
//! recharges past the comparator threshold — the elapsed time depends on the
//! paddle's resistance (its physical position), which is exactly what makes
//! this an analog input rather than a digital one.
//!
//! This is a direct, faithful port of Stella's `AnalogReadout`
//! (`src/emucore/tia/AnalogReadout.{cxx,hxx}`) — same RC constants, same
//! exponential charge/discharge formulas, same comparator semantics. Only
//! NTSC timing is modeled (`CLOCK_FREQ`/`TRIPPOINT_LINES` assume 60Hz/262
//! lines); this crate has no region concept of its own (that's a
//! frontend/palette-level distinction — see `rusty2600_frontend::palette::Region`),
//! so PAL/SECAM-specific comparator-threshold behavior is not modeled and is
//! honestly unverified rather than guessed.

/// 1.8kΩ series resistor in the TIA's analog input circuit.
const R0: f64 = 1800.0;
/// 68nF timing capacitor.
const CAP: f64 = 68e-9;
/// 50Ω dump resistor (grounds the capacitor when `VBLANK` bit 7 is set).
const R_DUMP: f64 = 50.0;
/// 5V supply voltage.
const U_SUPP: f64 = 5.0;
/// Maximum paddle pot resistance (1MΩ) — a paddle's position maps linearly
/// onto `0..=MAX_POT_RESISTANCE`.
pub const MAX_POT_RESISTANCE: f64 = 1_000_000.0;
/// Comparator trip point, in scanline units (NTSC).
const TRIPPOINT_LINES: f64 = 379.0;
/// NTSC color clocks per second: 60 fps * 228 color-clocks/line * 262 lines/frame.
const NTSC_CLOCK_FREQ: f64 = 60.0 * 228.0 * 262.0;

/// Map a host-side `0..=255` paddle position (`0` = fully clockwise, `255` =
/// fully counter-clockwise, matching every existing `Paddle`/`MobilePaddle`
/// doc comment in this project) onto the pot's physical resistance in ohms.
#[must_use]
pub fn position_to_resistance(position: u8) -> f64 {
    (f64::from(position) / 255.0) * MAX_POT_RESISTANCE
}

/// How the analog input pin is currently wired.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Connection {
    /// Capacitor charges toward `U_SUPP` through `R0 + resistance`— an
    /// active, connected paddle.
    Vcc {
        /// The paddle's current pot resistance, in ohms.
        resistance: f64,
    },
    /// Capacitor holds its current charge — no paddle connected.
    Disconnected,
}

impl Default for Connection {
    fn default() -> Self {
        Self::Disconnected
    }
}

/// One `INPTx` channel's full RC-circuit simulation state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnalogPaddle {
    /// Current capacitor voltage.
    u: f64,
    /// Comparator threshold voltage (precomputed for NTSC timing).
    u_thresh: f64,
    /// How the pin is currently connected.
    connection: Connection,
    /// The `paddle_clock` value (see `Tia::paddle_clock`) as of the last
    /// charge-simulation update.
    last_timestamp: u64,
    /// Whether the `VBLANK` dump (ground discharge) is currently active.
    is_dumped: bool,
}

impl Default for AnalogPaddle {
    fn default() -> Self {
        Self {
            u: 0.0,
            u_thresh: ntsc_threshold(),
            connection: Connection::default(),
            last_timestamp: 0,
            is_dumped: false,
        }
    }
}

/// The comparator threshold voltage at NTSC timing — the capacitor voltage
/// `INPTx` trips high at, calibrated against the worst-case (maximum
/// resistance) charge time in scanline units.
fn ntsc_threshold() -> f64 {
    U_SUPP
        * (1.0
            - libm::exp(
                -TRIPPOINT_LINES * 228.0 / NTSC_CLOCK_FREQ / (MAX_POT_RESISTANCE + R0) / CAP,
            ))
}

impl AnalogPaddle {
    /// Update this paddle's pot resistance (an active, connected paddle is
    /// always in the `Vcc` state — a real paddle is either plugged in and
    /// charging, or absent).
    pub fn set_position(&mut self, position: u8, timestamp: u64) {
        self.update_charge(timestamp);
        self.connection = Connection::Vcc {
            resistance: position_to_resistance(position),
        };
    }

    /// Process a `VBLANK` write. Bit 7 controls the ground dump: set grounds
    /// the capacitor through `R_DUMP`, clearing it releases the dump and lets
    /// the capacitor resume charging from wherever it was.
    pub fn vblank(&mut self, value: u8, timestamp: u64) {
        self.update_charge(timestamp);

        let was_dumped = self.is_dumped;
        if value & 0x80 != 0 {
            self.is_dumped = true;
        } else if was_dumped {
            self.is_dumped = false;
        }
        self.last_timestamp = timestamp;
    }

    /// Read this channel's `INPTx` value: `0x80` once the capacitor has
    /// charged past the comparator threshold, `0x00` while charging or
    /// dumped.
    #[must_use]
    pub fn inpt(&mut self, timestamp: u64) -> u8 {
        self.update_charge(timestamp);
        let high = !self.is_dumped && self.u > self.u_thresh;
        if high { 0x80 } else { 0x00 }
    }

    /// Advance the RC charge/discharge simulation from `last_timestamp` to
    /// `timestamp` (both in TIA color-clock units).
    fn update_charge(&mut self, timestamp: u64) {
        let elapsed = timestamp.wrapping_sub(self.last_timestamp) as f64;

        if self.is_dumped {
            self.u *= libm::exp(-elapsed / R_DUMP / CAP / NTSC_CLOCK_FREQ);
        } else if let Connection::Vcc { resistance } = self.connection {
            self.u = U_SUPP
                * (1.0
                    - (1.0 - self.u / U_SUPP)
                        * libm::exp(-elapsed / (resistance + R0) / CAP / NTSC_CLOCK_FREQ));
        }
        // `Connection::Disconnected` holds its charge — nothing to do.

        self.last_timestamp = timestamp;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_paddle_reads_low() {
        let mut p = AnalogPaddle::default();
        assert_eq!(p.inpt(0), 0x00);
    }

    #[test]
    fn min_resistance_charges_faster_than_max_resistance() {
        let mut low = AnalogPaddle::default();
        low.set_position(0, 0); // 0 ohms — fastest charge
        let mut high = AnalogPaddle::default();
        high.set_position(255, 0); // ~1MΩ — slowest charge

        // Advance both the same amount and confirm the low-resistance
        // paddle's capacitor voltage is strictly ahead.
        let t = 50_000u64;
        let _ = low.inpt(t);
        let _ = high.inpt(t);
        assert!(low.u > high.u, "low={}  high={}", low.u, high.u);
    }

    #[test]
    fn min_resistance_trips_high_before_max_resistance() {
        let mut low = AnalogPaddle::default();
        low.set_position(0, 0);
        let mut high = AnalogPaddle::default();
        high.set_position(255, 0);

        // Find the first timestamp (color clocks) at which `low` trips high.
        let mut trip_low = None;
        let mut trip_high = None;
        for t in (0..2_000_000u64).step_by(1000) {
            if trip_low.is_none() && low.inpt(t) == 0x80 {
                trip_low = Some(t);
            }
            if trip_high.is_none() && high.inpt(t) == 0x80 {
                trip_high = Some(t);
            }
            if trip_low.is_some() && trip_high.is_some() {
                break;
            }
        }
        let trip_low = trip_low.expect("min-resistance paddle should trip within range");
        let trip_high = trip_high.expect("max-resistance paddle should trip within range");
        assert!(trip_low < trip_high);
    }

    #[test]
    fn dump_resets_capacitor_toward_zero() {
        let mut p = AnalogPaddle::default();
        p.set_position(0, 0);
        // Charge for a while, confirm it trips high.
        assert_eq!(p.inpt(1_000_000), 0x80);
        // Dump it (VBLANK bit 7 set). While dumped, INPTx always reads low
        // regardless of capacitor voltage (the `!self.is_dumped` short
        // circuit), so this is already a meaningful check even a single
        // clock after the dump starts.
        p.vblank(0x80, 1_000_001);
        assert_eq!(p.inpt(1_000_002), 0x00);
        // Give the R_DUMP*C time constant (~12 color clocks) many multiples
        // to actually discharge the capacitor toward 0 before releasing.
        let u_after_dump = {
            let _ = p.inpt(1_000_500);
            p.u
        };
        assert!(
            u_after_dump < 0.01,
            "capacitor should be near-fully discharged after ~500 clocks of dump, got {u_after_dump}"
        );
        // Release the dump; a fresh charge cycle should need time again.
        p.vblank(0x00, 1_000_501);
        assert_eq!(p.inpt(1_000_502), 0x00);
    }

    #[test]
    fn disconnected_paddle_holds_charge() {
        let mut p = AnalogPaddle::default();
        // Never call set_position — stays Disconnected — reading over time
        // should never trip high since the capacitor never charges.
        assert_eq!(p.inpt(10_000_000), 0x00);
    }

    #[test]
    fn position_to_resistance_matches_the_full_scale_convention() {
        assert!((position_to_resistance(0) - 0.0).abs() < f64::EPSILON);
        assert!((position_to_resistance(255) - MAX_POT_RESISTANCE).abs() < 1.0);
    }
}
