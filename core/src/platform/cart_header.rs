//! One-shot fixed-ROM header acquisition. Consumes the reader: no retry or MBC writes.
use super::rom_reader::{ReadBus, ReadError, RomReader};

pub const HEADER_START: u16 = 0x0134;
pub const HEADER_LEN: usize = 26;

#[derive(Debug, PartialEq, Eq)]
pub struct CartHeader(pub [u8; HEADER_LEN]);

#[derive(Debug, PartialEq, Eq)]
pub enum HeaderError<E> {
    Read { address: u16, error: ReadError<E> },
    Checksum,
    UnexpectedType(u8),
    UnexpectedRomSize(u8),
    UnexpectedRamSize(u8),
}

/// Kirby 2 expected profile (03/04/02) is an assumption to check, not proof of PCB identity.
pub fn read_header<B: ReadBus>(
    mut reader: RomReader<B>,
) -> Result<CartHeader, HeaderError<B::Error>> {
    let mut bytes = [0; HEADER_LEN];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let address = HEADER_START + index as u16;
        *byte = reader
            .read(address)
            .map_err(|error| HeaderError::Read { address, error })?;
    }
    let checksum = bytes[..25]
        .iter()
        .fold(0u8, |sum, b| sum.wrapping_sub(*b).wrapping_sub(1));
    if checksum != bytes[25] {
        return Err(HeaderError::Checksum);
    }
    if bytes[19] != 3 {
        return Err(HeaderError::UnexpectedType(bytes[19]));
    }
    if bytes[20] != 4 {
        return Err(HeaderError::UnexpectedRomSize(bytes[20]));
    }
    if bytes[21] != 2 {
        return Err(HeaderError::UnexpectedRamSize(bytes[21]));
    }
    Ok(CartHeader(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::rom_reader::{Operation, ReadTiming};
    use std::{cell::RefCell, rc::Rc, vec::Vec};
    struct Bus {
        bytes: [u8; 26],
        addresses: Rc<RefCell<Vec<u16>>>,
        fail: Option<u16>,
    }
    impl ReadBus for Bus {
        type Error = ();
        fn perform(&mut self, op: Operation) -> Result<u8, ()> {
            match op {
                Operation::Address(a) => self.addresses.borrow_mut().push(a),
                Operation::Sample => {
                    let a = *self.addresses.borrow().last().unwrap();
                    if self.fail == Some(a) {
                        return Err(());
                    }
                    return Ok(self.bytes[(a - HEADER_START) as usize]);
                }
                _ => {}
            }
            Ok(0)
        }
    }
    fn bytes() -> [u8; 26] {
        let mut b = [0; 26];
        b[19] = 3;
        b[20] = 4;
        b[21] = 2;
        checksum(&mut b);
        b
    }
    fn checksum(b: &mut [u8; 26]) {
        b[25] = b[..25]
            .iter()
            .fold(0u8, |s, v| s.wrapping_sub(*v).wrapping_sub(1));
    }
    fn reader(b: [u8; 26], fail: Option<u16>, addresses: Rc<RefCell<Vec<u16>>>) -> RomReader<Bus> {
        RomReader::new(
            Bus {
                bytes: b,
                addresses,
                fail,
            },
            ReadTiming {
                disable: 1,
                address: 1,
                direction: 1,
                enable: 1,
                release: 1,
            },
            true,
        )
        .unwrap()
    }
    #[test]
    fn exactly_26_sequential_reads() {
        let addresses = Rc::default();
        assert_eq!(
            read_header(reader(bytes(), None, Rc::clone(&addresses))),
            Ok(CartHeader(bytes()))
        );
        assert_eq!(*addresses.borrow(), (0x134..=0x14d).collect::<Vec<_>>());
    }
    #[test]
    fn stops_at_every_first_read_failure() {
        for a in 0x134..=0x14d {
            let addresses = Rc::default();
            assert!(
                matches!(read_header(reader(bytes(),Some(a),Rc::clone(&addresses))),Err(HeaderError::Read {address,..}) if address==a)
            );
            assert_eq!(*addresses.borrow(), (0x134..=a).collect::<Vec<_>>());
        }
    }
    #[test]
    fn rejects_checksum_and_each_profile_field() {
        let mut b = bytes();
        b[25] ^= 1;
        assert_eq!(
            read_header(reader(b, None, Rc::default())),
            Err(HeaderError::Checksum)
        );
        for (index, error) in [
            (19, HeaderError::UnexpectedType(0)),
            (20, HeaderError::UnexpectedRomSize(0)),
            (21, HeaderError::UnexpectedRamSize(0)),
        ] {
            let mut b = bytes();
            b[index] = 0;
            checksum(&mut b);
            assert_eq!(read_header(reader(b, None, Rc::default())), Err(error));
        }
    }
}
