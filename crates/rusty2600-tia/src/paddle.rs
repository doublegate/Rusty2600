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

    // -- Stella `AnalogReadout` oracle differential (v2.4.0, T-0501-010 follow-up) --
    //
    // Independently re-derives Stella's exact RC-circuit formulas
    // (`ref-proj/stella/src/emucore/tia/AnalogReadout.{cxx,hxx}`, `AnalogReadout::
    // setConsoleTiming`/`updateCharge`/`inpt`) in this test module only — NOT
    // shared with the production code above, so this is a genuine independent
    // check, not a tautology against our own implementation. Verified by hand
    // against Stella's source, line-by-line, before writing this module:
    //
    //   myUThresh = U_SUPP * (1 - exp(-TRIPPOINT_LINES*228 / myClockFreq
    //                                 / (MAX_POT_RESISTANCE + R0) / C))
    //   charging:  myU = U_SUPP * (1 - (1 - myU/U_SUPP)
    //                               * exp(-dt / (resistance + R0) / C / myClockFreq))
    //   dumped:    myU *= exp(-dt / R_DUMP / C / myClockFreq)
    //
    // All four RC constants (R0=1800, C=68e-9, R_DUMP=50, U_SUPP=5), TRIPPOINT_LINES
    // (379) and NTSC_CLOCK_FREQ (60*228*262) already match `paddle.rs`'s constants
    // above exactly (confirmed by direct comparison against `AnalogReadout.hxx`).
    //
    // One deliberate, confirmed-correct divergence: Stella's `ConnectionType` has
    // a third `ground` variant (capacitor discharges through `resistance + R0`,
    // distinct from the VBLANK `myIsDumped` path which always uses `R_DUMP`).
    // Grepping Stella's own `Paddles.cxx` (the actual paddle controller class this
    // module claims fidelity to) shows it NEVER calls `AnalogReadout::
    // connectToGround()` — only `connectToVcc()`/`disconnect()`. `connectToGround`
    // is exclusively used by unrelated non-paddle controllers (Booster, CompuMate,
    // Genesis, Joy2BPlus, Keyboard, QuadTari) this crate does not model. So
    // `Connection`'s two-variant (`Vcc`/`Disconnected`) shape here is a correct,
    // deliberate scope match to real paddle hardware, not a missing feature.
    //
    // A second, inconsequential shape difference: Stella's `update()` only calls
    // `updateCharge` (i.e. advances the RC integration) when the new `Connection`
    // differs from the old one; `AnalogPaddle::set_position` above always does.
    // This does not change the simulated result: RC charging under a fixed
    // resistance is composable (`exp(-t1/RC) * exp(-t2/RC) == exp(-(t1+t2)/RC)`),
    // so re-integrating across an unbroken same-resistance span in two calls
    // versus one gives the identical final capacitor voltage — confirmed by the
    // `set_position_called_redundantly_matches_stella_early_exit_optimization`
    // test below, which exercises exactly this shape difference.
    mod stella_oracle {
        //! A from-scratch re-implementation of `AnalogReadout`'s formulas, used
        //! ONLY to cross-check `super::AnalogPaddle` — never imported by
        //! production code.
        const R0: f64 = 1800.0;
        const CAP: f64 = 68e-9;
        const R_DUMP: f64 = 50.0;
        const U_SUPP: f64 = 5.0;
        const TRIPPOINT_LINES: f64 = 379.0;
        const NTSC_CLOCK_FREQ: f64 = 60.0 * 228.0 * 262.0;

        pub fn u_thresh() -> f64 {
            U_SUPP
                * (1.0
                    - libm::exp(
                        -TRIPPOINT_LINES * 228.0
                            / NTSC_CLOCK_FREQ
                            / (super::MAX_POT_RESISTANCE + R0)
                            / CAP,
                    ))
        }

        /// Charges (or discharges, if `dumped`) `u` across `elapsed` color
        /// clocks at a fixed `resistance`, mirroring `AnalogReadout::updateCharge`.
        pub fn advance(u: f64, elapsed: f64, resistance: f64, dumped: bool) -> f64 {
            if dumped {
                u * libm::exp(-elapsed / R_DUMP / CAP / NTSC_CLOCK_FREQ)
            } else {
                U_SUPP
                    * (1.0
                        - (1.0 - u / U_SUPP)
                            * libm::exp(-elapsed / (resistance + R0) / CAP / NTSC_CLOCK_FREQ))
            }
        }
    }

    /// The precomputed NTSC comparator threshold must match the oracle exactly
    /// (both sides evaluate the identical closed-form expression, so this
    /// should be bit-for-bit, not just approximately equal).
    #[test]
    fn threshold_matches_stella_oracle() {
        let p = AnalogPaddle::default();
        assert!(
            (p.u_thresh - stella_oracle::u_thresh()).abs() < 1e-15,
            "port={}  oracle={}",
            p.u_thresh,
            stella_oracle::u_thresh()
        );
    }

    /// Sweeps a representative range of paddle positions (`0`, `64`, `128`,
    /// `192`, `255` — spanning `position_to_resistance`'s documented `0..=255`
    /// range) and confirms the port's capacitor voltage after a single charge
    /// step matches the independently re-derived Stella formula exactly, for
    /// several elapsed-time magnitudes (short/medium/long relative to the RC
    /// time constant at each resistance).
    #[test]
    fn single_step_charge_matches_stella_oracle_across_positions() {
        for position in [0u8, 64, 128, 192, 255] {
            let resistance = position_to_resistance(position);
            for &elapsed in &[10.0, 1_000.0, 100_000.0, 2_000_000.0] {
                let mut p = AnalogPaddle::default();
                p.set_position(position, 0);
                // `set_position` already advanced charge to t=0 (a no-op,
                // elapsed=0); now advance to `elapsed` and compare.
                let _ = p.inpt(elapsed as u64);
                let expected = stella_oracle::advance(0.0, elapsed, resistance, false);
                assert!(
                    (p.u - expected).abs() < 1e-9,
                    "position={position} elapsed={elapsed}: port={} oracle={expected}",
                    p.u
                );
            }
        }
    }

    /// Confirms multi-step charging (several successive `inpt()` calls at
    /// increasing timestamps, as a real CPU polling loop would do) matches the
    /// oracle applied incrementally the same way — not just a single big jump.
    #[test]
    fn multi_step_charge_matches_stella_oracle() {
        let position = 96u8;
        let resistance = position_to_resistance(position);
        let mut p = AnalogPaddle::default();
        p.set_position(position, 0);

        let mut oracle_u = 0.0;
        let mut oracle_t = 0.0f64;
        for step_t in [5_000.0, 20_000.0, 75_000.0, 300_000.0, 900_000.0] {
            let _ = p.inpt(step_t as u64);
            oracle_u = stella_oracle::advance(oracle_u, step_t - oracle_t, resistance, false);
            oracle_t = step_t;
            assert!(
                (p.u - oracle_u).abs() < 1e-9,
                "at t={step_t}: port={} oracle={oracle_u}",
                p.u
            );
        }
    }

    /// Confirms the VBLANK-dump discharge path matches the oracle too (a
    /// separate code branch from charging — `R_DUMP`, not the pot resistance).
    #[test]
    fn dump_discharge_matches_stella_oracle() {
        let mut p = AnalogPaddle::default();
        p.set_position(0, 0); // fastest charge, to get `u` well above 0 first.
        let _ = p.inpt(1_000_000);
        let u_before_dump = p.u;

        p.vblank(0x80, 1_000_000);
        let _ = p.inpt(1_000_300);

        let expected = stella_oracle::advance(u_before_dump, 300.0, 0.0, true);
        assert!(
            (p.u - expected).abs() < 1e-9,
            "port={} oracle={expected}",
            p.u
        );
    }

    /// Confirms the shape difference documented above (`set_position` always
    /// re-integrates vs. Stella's `update()` skipping when the connection is
    /// unchanged) produces an IDENTICAL result, not a divergence — calling
    /// `set_position` redundantly with the same position mid-charge must land
    /// on the same voltage as never having called it again at all.
    #[test]
    fn set_position_called_redundantly_matches_stella_early_exit_optimization() {
        let position = 200u8;

        let mut redundant = AnalogPaddle::default();
        redundant.set_position(position, 0);
        let _ = redundant.inpt(50_000);
        redundant.set_position(position, 50_000); // same position, re-set mid-charge
        let _ = redundant.inpt(150_000);

        let mut untouched = AnalogPaddle::default();
        untouched.set_position(position, 0);
        let _ = untouched.inpt(150_000);

        assert!(
            (redundant.u - untouched.u).abs() < 1e-9,
            "redundant={} untouched={}",
            redundant.u,
            untouched.u
        );
    }
}
