//! The memory seam a future cartridge `Board` implements to host this
//! interpreter, mirroring Gopher2600's `SharedMemory`/fault taxonomy.

use crate::Arm7Tdmi;

/// A memory fault: an access the interpreter could not service.
///
/// Mirrors Gopher2600's `coprocessor/faults.Category` taxonomy (minus the
/// program-memory-specific variant, which belongs to the execution-cache
/// machinery this crate doesn't implement — see the crate-level docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// A read/write address `ThumbMemory::map` couldn't resolve, and none of
    /// the peripheral fallbacks recognized it either.
    IllegalAddress(u32),
    /// The address is architecturally reserved but not implemented in this
    /// interpreter (peripheral registers a future `Board` will add).
    UnimplementedPeripheral(u32),
    /// An access below the memory map's null-pointer guard boundary.
    NullDereference(u32),
    /// An access that required N-byte alignment wasn't aligned.
    Misaligned(u32),
}

/// The memory a [`Arm7Tdmi`] executes against — the seam a future
/// cartridge `Board` (DPC+/CDF/CDFJ/CDFJ+) will implement.
///
/// Mirrors Gopher2600's `arm.SharedMemory` interface. Deliberately generic:
/// it says nothing about any specific coprocessor's register map (that's
/// Gopher2600's `architecture.Map`, itself cartridge-specific) — a future
/// `Board` supplies its own.
pub trait ThumbMemory {
    /// Map `addr` to a mutable byte slice plus that slice's origin address,
    /// or `None` if `addr` isn't backed by plain memory (RAM/flash) — in
    /// which case the interpreter tries peripheral-register fallbacks
    /// (`mam_read`/`mam_write` for now; RNG/timer registers are a future
    /// `Board`'s concern, see the crate-level docs) before reporting a
    /// [`Fault`].
    ///
    /// `write` and `executing` give the same context Gopher2600's
    /// implementation uses: `executing` is true only when the interpreter
    /// is fetching an instruction opcode (as opposed to a data access),
    /// which a `Board` can use to enforce execute-only or data-only regions.
    fn map(&mut self, addr: u32, write: bool, executing: bool) -> Option<(&mut [u8], u32)>;

    /// Reset vectors: `(stack pointer, link register, program counter)`.
    fn reset_vectors(&self) -> (u32, u32, u32);

    /// Whether `addr` contains executable instructions (a `Board` may
    /// restrict execution to flash/ROM regions only).
    fn is_executable(&self, addr: u32) -> bool;

    /// The lowest address considered a valid (non-null) access. Defaults to
    /// `0`, i.e. no null-guard — a `Board` targeting a real memory map
    /// (Gopher2600's LPC2000-alike maps guard the bottom of the address
    /// space) can override this.
    fn null_access_boundary(&self) -> u32 {
        0
    }

    /// Whether misaligned 16/32-bit accesses are tolerated (and simply
    /// realigned) rather than faulted. Defaults to `false` (fault), matching
    /// the ARM7TDMI's real behavior — misaligned Thumb code is a genuine bug.
    fn tolerates_misaligned_access(&self) -> bool {
        false
    }
}

impl<M: ThumbMemory> Arm7Tdmi<M> {
    fn check_null(&self, addr: u32) -> Result<(), Fault> {
        if addr < self.mem.null_access_boundary() {
            Err(Fault::NullDereference(addr))
        } else {
            Ok(())
        }
    }

    fn check_alignment(&self, addr: u32, align: u32) -> Result<u32, Fault> {
        let mask = align - 1;
        if addr & mask == 0 {
            return Ok(addr);
        }
        if self.mem.tolerates_misaligned_access() {
            Ok(addr)
        } else {
            Err(Fault::Misaligned(addr))
        }
    }

    pub(crate) fn read8(&mut self, addr: u32) -> Result<u8, Fault> {
        self.check_null(addr)?;
        match self.mem.map(addr, false, false) {
            Some((bytes, origin)) => {
                let idx = (addr - origin) as usize;
                bytes.get(idx).copied().ok_or(Fault::IllegalAddress(addr))
            }
            None => self.peripheral_read(addr).map(|v| v as u8),
        }
    }

    pub(crate) fn write8(&mut self, addr: u32, val: u8) -> Result<(), Fault> {
        self.check_null(addr)?;
        match self.mem.map(addr, true, false) {
            Some((bytes, origin)) => {
                let idx = (addr - origin) as usize;
                if let Some(slot) = bytes.get_mut(idx) {
                    *slot = val;
                    Ok(())
                } else {
                    Err(Fault::IllegalAddress(addr))
                }
            }
            None => self.peripheral_write(addr, u32::from(val)),
        }
    }

    pub(crate) fn read16(&mut self, addr: u32, requires_alignment: bool) -> Result<u16, Fault> {
        self.check_null(addr)?;
        let addr = if requires_alignment || !self.mem.tolerates_misaligned_access() {
            self.check_alignment(addr, 2)?
        } else {
            addr
        };
        match self.mem.map(addr, false, false) {
            Some((bytes, origin)) => {
                let idx = (addr - origin) as usize;
                let b = bytes.get(idx..idx + 2).ok_or(Fault::IllegalAddress(addr))?;
                Ok(u16::from_le_bytes([b[0], b[1]]))
            }
            None => self.peripheral_read(addr).map(|v| v as u16),
        }
    }

    pub(crate) fn write16(
        &mut self,
        addr: u32,
        val: u16,
        requires_alignment: bool,
    ) -> Result<(), Fault> {
        self.check_null(addr)?;
        let addr = if requires_alignment || !self.mem.tolerates_misaligned_access() {
            self.check_alignment(addr, 2)?
        } else {
            addr
        };
        match self.mem.map(addr, true, false) {
            Some((bytes, origin)) => {
                let idx = (addr - origin) as usize;
                let slot = bytes
                    .get_mut(idx..idx + 2)
                    .ok_or(Fault::IllegalAddress(addr))?;
                slot.copy_from_slice(&val.to_le_bytes());
                Ok(())
            }
            None => self.peripheral_write(addr, u32::from(val)),
        }
    }

    pub(crate) fn read32(&mut self, addr: u32, requires_alignment: bool) -> Result<u32, Fault> {
        self.check_null(addr)?;
        let addr = if requires_alignment || !self.mem.tolerates_misaligned_access() {
            self.check_alignment(addr, 4)?
        } else {
            addr
        };
        match self.mem.map(addr, false, false) {
            Some((bytes, origin)) => {
                let idx = (addr - origin) as usize;
                let b = bytes.get(idx..idx + 4).ok_or(Fault::IllegalAddress(addr))?;
                Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            }
            None => self.peripheral_read(addr),
        }
    }

    pub(crate) fn write32(
        &mut self,
        addr: u32,
        val: u32,
        requires_alignment: bool,
    ) -> Result<(), Fault> {
        self.check_null(addr)?;
        let addr = if requires_alignment || !self.mem.tolerates_misaligned_access() {
            self.check_alignment(addr, 4)?
        } else {
            addr
        };
        match self.mem.map(addr, true, false) {
            Some((bytes, origin)) => {
                let idx = (addr - origin) as usize;
                let slot = bytes
                    .get_mut(idx..idx + 4)
                    .ok_or(Fault::IllegalAddress(addr))?;
                slot.copy_from_slice(&val.to_le_bytes());
                Ok(())
            }
            None => self.peripheral_write(addr, val),
        }
    }

    fn peripheral_read(&mut self, addr: u32) -> Result<u32, Fault> {
        if let Some(v) = self.mam.read(addr) {
            return Ok(v);
        }
        Err(Fault::IllegalAddress(addr))
    }

    fn peripheral_write(&mut self, addr: u32, val: u32) -> Result<(), Fault> {
        if self.mam.write(addr, val) {
            return Ok(());
        }
        Err(Fault::IllegalAddress(addr))
    }
}
