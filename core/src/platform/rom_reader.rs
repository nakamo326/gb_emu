//! GPIO-free, fail-closed fixed-ROM reader. No production GPIO backend is provided.

/// Backend operations. Implementations must finish each operation before returning.
/// Even Err may mean the physical operation occurred. `IdleInputs` must verify MCU
/// data inputs and inactive /WR; it must never pulse /WR or reset the cartridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    DataEnabled(bool),
    Wait(u32),
    IdleInputs,
    Address(u16),
    ReadAsserted(bool),
    Sample,
}

pub trait ReadBus {
    type Error;
    fn perform(&mut self, operation: Operation) -> Result<u8, Self::Error>;
}

/// Backend-defined wait units, not hardware-certified nanoseconds. All values must
/// be nonzero. Choose worst-case limits for disable, address, DIR, enable and cart release.
#[derive(Clone, Copy, Debug)]
pub struct ReadTiming {
    pub disable: u32,
    pub address: u32,
    pub direction: u32,
    pub enable: u32,
    pub release: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReadError<E> {
    GateClosed,
    InvalidTiming,
    Faulted,
    OutsideFixedRom,
    Bus {
        cause: E,
        isolation: Option<E>,
        wait: Option<E>,
    },
}

/// The caller must explicitly attest the test setup is ready. This software gate
/// does not establish electrical safety. Initial /RD must be inactive, data must
/// be input, and hardware must keep /WR inactive. There is no fault-clear method.
pub struct RomReader<B> {
    bus: B,
    timing: ReadTiming,
    faulted: bool,
}

impl<B: ReadBus> RomReader<B> {
    pub fn new(bus: B, timing: ReadTiming, ready: bool) -> Result<Self, ReadError<B::Error>> {
        if !ready {
            return Err(ReadError::GateClosed);
        }
        if [
            timing.disable,
            timing.address,
            timing.direction,
            timing.enable,
            timing.release,
        ]
        .contains(&0)
        {
            return Err(ReadError::InvalidTiming);
        }
        Ok(Self {
            bus,
            timing,
            faulted: false,
        })
    }

    fn step(&mut self, op: Operation) -> Result<u8, ReadError<B::Error>> {
        self.bus.perform(op).map_err(|cause| {
            self.faulted = true;
            // Always attempt BOTH isolation and its wait, even if isolation fails.
            // Do not change /RD (= DIR) after a failure, including a Sample failure.
            let isolation = self.bus.perform(Operation::DataEnabled(false)).err();
            let wait = self.bus.perform(Operation::Wait(self.timing.disable)).err();
            ReadError::Bus {
                cause,
                isolation,
                wait,
            }
        })
    }

    pub fn read(&mut self, address: u16) -> Result<u8, ReadError<B::Error>> {
        if self.faulted {
            return Err(ReadError::Faulted);
        }
        if address > 0x3fff {
            return Err(ReadError::OutsideFixedRom);
        }
        use Operation::*;
        self.step(DataEnabled(false))?;
        self.step(Wait(self.timing.disable))?;
        self.step(IdleInputs)?;
        self.step(Address(address))?;
        self.step(Wait(self.timing.address))?;
        self.step(ReadAsserted(true))?;
        self.step(Wait(self.timing.direction))?;
        self.step(DataEnabled(true))?;
        self.step(Wait(self.timing.enable))?;
        let value = self.step(Sample)?;
        self.step(DataEnabled(false))?;
        self.step(Wait(self.timing.disable))?;
        self.step(ReadAsserted(false))?;
        self.step(Wait(self.timing.release))?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc, vec::Vec};
    const T: ReadTiming = ReadTiming {
        disable: 1,
        address: 2,
        direction: 3,
        enable: 4,
        release: 5,
    };
    struct Mock {
        calls: Rc<RefCell<Vec<Operation>>>,
        applied: Rc<RefCell<Vec<Operation>>>,
        failures: Vec<usize>,
        after: bool,
    }
    impl ReadBus for Mock {
        type Error = usize;
        fn perform(&mut self, op: Operation) -> Result<u8, usize> {
            let n = self.calls.borrow().len();
            self.calls.borrow_mut().push(op);
            let fail = self.failures.contains(&n);
            if !fail || self.after {
                self.applied.borrow_mut().push(op);
            }
            if fail { Err(n) } else { Ok(0x42) }
        }
    }
    fn mock(failures: Vec<usize>, after: bool) -> Mock {
        Mock {
            calls: Rc::default(),
            applied: Rc::default(),
            failures,
            after,
        }
    }
    fn sequence() -> Vec<Operation> {
        use Operation::*;
        vec![
            DataEnabled(false),
            Wait(1),
            IdleInputs,
            Address(0x134),
            Wait(2),
            ReadAsserted(true),
            Wait(3),
            DataEnabled(true),
            Wait(4),
            Sample,
            DataEnabled(false),
            Wait(1),
            ReadAsserted(false),
            Wait(5),
        ]
    }
    #[test]
    fn exact_sequence_and_repeated_reads() {
        let bus = mock(vec![], false);
        let calls = bus.calls.clone();
        let mut r = RomReader::new(bus, T, true).unwrap();
        assert_eq!(r.read(0x134), Ok(0x42));
        assert_eq!(*calls.borrow(), sequence());
        assert_eq!(r.read(0x134), Ok(0x42));
        assert_eq!(calls.borrow().len(), 28);
    }
    #[test]
    fn every_operation_fails_before_and_after_effect_and_latches_fault() {
        for after in [false, true] {
            for index in 0..14 {
                let bus = mock(vec![index], after);
                let calls = bus.calls.clone();
                let applied = bus.applied.clone();
                let mut r = RomReader::new(bus, T, true).unwrap();
                assert_eq!(
                    r.read(0x134),
                    Err(ReadError::Bus {
                        cause: index,
                        isolation: None,
                        wait: None
                    })
                );
                let mut expected = sequence()[..=index].to_vec();
                expected.extend([Operation::DataEnabled(false), Operation::Wait(1)]);
                assert_eq!(*calls.borrow(), expected);
                assert_eq!(applied.borrow().len(), expected.len() - usize::from(!after));
                assert_eq!(r.read(0x134), Err(ReadError::Faulted));
                assert_eq!(*calls.borrow(), expected);
            }
        }
    }
    #[test]
    fn failed_isolation_still_waits_and_preserves_all_errors() {
        for failures in [vec![9, 10], vec![9, 11], vec![9, 10, 11]] {
            let bus = mock(failures.clone(), true);
            let calls = bus.calls.clone();
            let mut r = RomReader::new(bus, T, true).unwrap();
            assert_eq!(
                r.read(0x134),
                Err(ReadError::Bus {
                    cause: 9,
                    isolation: failures.contains(&10).then_some(10),
                    wait: failures.contains(&11).then_some(11)
                })
            );
            assert_eq!(
                &calls.borrow()[10..],
                &[Operation::DataEnabled(false), Operation::Wait(1)]
            );
            assert_eq!(r.read(0), Err(ReadError::Faulted));
        }
    }
    #[test]
    fn gates_and_fixed_rom_bounds_touch_no_bus() {
        let bus = mock(vec![], false);
        let calls = bus.calls.clone();
        assert!(matches!(
            RomReader::new(bus, T, false),
            Err(ReadError::GateClosed)
        ));
        assert!(calls.borrow().is_empty());
        let bus = mock(vec![], false);
        let calls = bus.calls.clone();
        assert!(matches!(
            RomReader::new(bus, ReadTiming { disable: 0, ..T }, true),
            Err(ReadError::InvalidTiming)
        ));
        assert!(calls.borrow().is_empty());
        let bus = mock(vec![], false);
        let calls = bus.calls.clone();
        let mut r = RomReader::new(bus, T, true).unwrap();
        assert_eq!(r.read(0x4000), Err(ReadError::OutsideFixedRom));
        assert!(calls.borrow().is_empty());
    }
}
