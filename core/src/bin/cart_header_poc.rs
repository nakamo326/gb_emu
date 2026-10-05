//! Synthetic host-only diagnostic. Contains no GPIO or hardware backend.
use gb_core::platform::cart_header::{HeaderError, read_header};
use gb_core::platform::rom_reader::Operation;
use gb_core::platform::{ReadBus, ReadTiming, RomReader};

struct Synthetic {
    bytes: [u8; 26],
    address: u16,
    fail: bool,
}
impl ReadBus for Synthetic {
    type Error = &'static str;
    fn perform(&mut self, operation: Operation) -> Result<u8, Self::Error> {
        match operation {
            Operation::Address(a) => self.address = a,
            Operation::Sample => {
                if self.fail && self.address == 0x140 {
                    return Err("synthetic read failure");
                }
                return Ok(self.bytes[(self.address - 0x134) as usize]);
            }
            _ => {}
        }
        Ok(0)
    }
}
fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2
        || args[0] != "--synthetic"
        || ![
            "normal",
            "checksum",
            "type",
            "rom-size",
            "ram-size",
            "read-failure",
            "gate-refusal",
        ]
        .contains(&args[1].as_str())
    {
        eprintln!(
            "host-only usage: cart_header_poc --synthetic normal|checksum|type|rom-size|ram-size|read-failure|gate-refusal; hardware is unsupported"
        );
        return 2.into();
    }
    let scenario = &args[1];
    let mut bytes = [0u8; 26];
    bytes[19] = 3;
    bytes[20] = 4;
    bytes[21] = 2;
    match scenario.as_str() {
        "type" => bytes[19] = 0,
        "rom-size" => bytes[20] = 0,
        "ram-size" => bytes[21] = 0,
        _ => {}
    }
    bytes[25] = bytes[..25]
        .iter()
        .fold(0u8, |s, b| s.wrapping_sub(*b).wrapping_sub(1));
    if scenario == "checksum" {
        bytes[25] ^= 1;
    }
    let bus = Synthetic {
        bytes,
        address: 0,
        fail: scenario == "read-failure",
    };
    let timing = ReadTiming {
        disable: 1,
        address: 1,
        direction: 1,
        enable: 1,
        release: 1,
    };
    let reader = match RomReader::new(bus, timing, scenario != "gate-refusal") {
        Ok(reader) => reader,
        Err(error) => {
            eprintln!("synthetic gate refused: {error:?}");
            return 3.into();
        }
    };
    match read_header(reader) {
        Ok(_) => {
            println!("synthetic only: 26-byte header OK; expected 03/04/02; no GPIO accessed");
            0.into()
        }
        Err(HeaderError::Read { address, error }) => {
            eprintln!("synthetic read stopped at {address:#06x}: {error:?}");
            4.into()
        }
        Err(error) => {
            eprintln!("synthetic validation failed: {error:?}");
            5.into()
        }
    }
}
