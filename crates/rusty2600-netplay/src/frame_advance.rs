//! Drives a [`System`] forward exactly one frame given a [`MovieFrame`] of
//! input, and [`combine`], which recombines two players' [`PortInput`]s into
//! one.
//!
//! [`advance_one_frame`] is a small, deliberate duplication of
//! `rusty2600-frontend::emu_thread::EmuCore::run_frame`'s core loop (feed the
//! RIOT/TIA ports, step instructions to the next VSYNC falling edge) — this
//! crate cannot depend on `rusty2600-frontend` (the crate graph is
//! one-directional: frontend depends on core/netplay, never the reverse), so
//! a rollback session needs its own copy of "how to advance one frame"
//! against the bare `rusty2600_core::System` it owns internally.

use rusty2600_core::{MovieFrame, System};

use crate::config::PortInput;

/// Packs one [`PortInput`]'s four directions into a `SWCHA` nibble, matching
/// `rusty2600-frontend::input::Joystick::swcha_nibble`'s bit-for-bit
/// convention (mirrored here, not imported, per the crate-dependency-
/// direction constraint the module docs explain): idle-high, a pressed
/// direction clears its bit.
const fn swcha_nibble(input: PortInput) -> u8 {
    let mut n = 0b1111u8;
    if input.up {
        n &= !0b0001;
    }
    if input.down {
        n &= !0b0010;
    }
    if input.left {
        n &= !0b0100;
    }
    if input.right {
        n &= !0b1000;
    }
    n
}

/// Recombine port 0's and port 1's confirmed/predicted [`PortInput`]s into
/// one [`MovieFrame`] ready to hand to [`advance_one_frame`].
///
/// Console switches are left at their idle default (all switches released,
/// Color mode) and paddles at their centered default — see `config.rs`'s
/// module doc for why those aren't modeled per-player in this release.
#[must_use]
pub fn combine(port0: PortInput, port1: PortInput) -> MovieFrame {
    let swcha = (swcha_nibble(port0) << 4) | swcha_nibble(port1);
    let mut joy_fire = 0u8;
    if port0.fire {
        joy_fire |= 0x01;
    }
    if port1.fire {
        joy_fire |= 0x02;
    }
    MovieFrame {
        swcha,
        joy_fire,
        ..MovieFrame::default()
    }
}

/// Applies `input`'s packed port bytes to `system`'s RIOT/TIA registers.
///
/// Then steps instructions until the next VSYNC 1->0 falling edge (the frame
/// boundary), matching the native frontend's own convention exactly so a
/// netplay session and a local play session advance identically frame for
/// frame.
pub fn advance_one_frame(system: &mut System, input: MovieFrame) {
    system.bus.riot.pins[0] = input.swcha;
    system.bus.riot.pins[1] = input.swchb;
    system.bus.tia.inpt[4] = if input.joy_fire & 0x01 != 0 {
        0x00
    } else {
        0x80
    };
    system.bus.tia.inpt[5] = if input.joy_fire & 0x02 != 0 {
        0x00
    } else {
        0x80
    };
    system.bus.tia.inpt[0] = input.paddle_pos[0];
    system.bus.tia.inpt[1] = input.paddle_pos[1];
    system.bus.tia.inpt[2] = input.paddle_pos[2];
    system.bus.tia.inpt[3] = input.paddle_pos[3];

    let mut old_vsync = system.bus.tia.objects.vsync;
    // Same safety timeout `emu_thread::run_frame` uses: a hung/misbehaving
    // program must never stall a rollback session forever.
    for _ in 0..200_000u32 {
        system.step_instruction();
        let vsync = system.bus.tia.objects.vsync;
        if (old_vsync & 0x02 != 0) && (vsync & 0x02 == 0) {
            break;
        }
        old_vsync = vsync;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advances_at_least_one_instruction() {
        // A blank ROM (all-zero RAM/ROM) never asserts a real VSYNC pattern,
        // so this just proves the function runs to its safety-timeout bound
        // without panicking, not real frame timing (covered by
        // `rusty2600-tia`'s own tests).
        let mut system = System::new(0);
        advance_one_frame(&mut system, MovieFrame::default());
        assert!(system.color_clocks() > 0);
    }

    #[test]
    fn combine_packs_both_ports_into_one_swcha_byte() {
        let port0 = PortInput {
            up: true,
            ..PortInput::default()
        };
        let port1 = PortInput {
            fire: true,
            ..PortInput::default()
        };
        let frame = combine(port0, port1);
        // Port 0 (high nibble): up pressed -> bit 0 of that nibble clear.
        assert_eq!(frame.swcha & 0xF0, 0b1110_0000);
        // Port 1 (low nibble): no direction pressed -> untouched, all high.
        assert_eq!(frame.swcha & 0x0F, 0b0000_1111);
        // Port 1's fire -> joy_fire bit 1; port 0's fire not pressed -> bit 0 clear.
        assert_eq!(frame.joy_fire, 0b10);
    }

    #[test]
    fn combine_idle_inputs_produce_the_idle_movie_frame() {
        let frame = combine(PortInput::default(), PortInput::default());
        assert_eq!(frame, MovieFrame::default());
    }
}
