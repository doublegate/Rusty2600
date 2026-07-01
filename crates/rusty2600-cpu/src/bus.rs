//! The narrow bus view the CPU borrows for the duration of an instruction.

/// The address-space bus the CPU sees. `rusty2600-core`'s `Bus` (which owns
/// the TIA, RIOT, and cart board) implements this via a thin adapter
/// (`CpuView`) so the CPU crate stays console-agnostic.
pub trait CpuBus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);

    /// Advance the rest of the system (TIA/RIOT/cart) by exactly one CPU
    /// cycle's worth of real time. Called once per bus access the CPU makes,
    /// so a multi-cycle instruction advances the world cycle-by-cycle instead
    /// of all at once — this is what keeps the CPU's notion of elapsed time in
    /// lockstep with the TIA's color clock. Default no-op for buses that don't
    /// model timing (flat-RAM test harnesses, the Klaus functional-test bus).
    fn tick_cycle(&mut self) {}

    /// Whether the bus is holding RDY low (the WSYNC beam-stall). The CPU
    /// spins on [`Self::tick_cycle`] while this is true instead of performing
    /// its next access. Default false for buses with no such concept.
    fn rdy_stall(&self) -> bool {
        false
    }
}
