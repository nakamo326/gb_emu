//! DMG BootROM (0x0000–0x00FF) の保持。
//! バイト列はプラットフォーム側が供給する（host=ファイル, teensy=`include_bytes!`）。

pub struct Bootrom {
    rom: [u8; 0x100],
    active: bool,
}

impl Bootrom {
    /// BootROM バイト列から有効状態で生成する。
    pub fn from_bytes(rom: [u8; 0x100]) -> Self {
        Self { rom, active: true }
    }

    /// BootROM 無効状態で生成する（DMG 初期値を別途適用する想定）。
    pub fn disabled() -> Self {
        Self { rom: [0; 0x100], active: false }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn write(&mut self, _: u16, val: u8) {
        self.active &= val == 0;
    }

    pub fn read(&self, addr: u16) -> u8 {
        self.rom[addr as usize]
    }
}
