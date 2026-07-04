use gb_core::platform::CartridgeBus;

/// ROM を CartridgeBus として提供するモジュール。
///
/// # Flash 埋め込み方式 (デフォルト)
///
/// `FlashCart` は `include_bytes!` でビルド時に Flash へ埋め込んだ ROM を使う。
/// SDカード回路が不要で、まず動作確認する際に使う。
///
/// # SDカード方式 (将来実装)
///
/// `teensy4-bsp 0.5` / `imxrt-hal 0.5` は embedded-hal 0.2 と 1.0 の両方を実装するため、
/// `embedded-sdmmc 0.7` (embedded-hal 1.0 必須) と組み合わせて実装可能。
/// SPI は `board::lpspi(...)` の戻り値を embedded-hal 1.0 の `SpiDevice` を満たすよう
/// ラップして渡す。

// 外部 RAM 最大サイズ。ポケモンクリスタル (MBC3, ram_size code 0x05 = 64KB) を含め
// 実測で使用する ROM のうち最大のものに合わせる (8 バンク × 8KB = 64KB)。
const MAX_RAM: usize = 0x10000;

// CART_RAM を OCRAM (uninit セクション) に置いて DTCM スタック予算を節約する。
// DMA の転送対象でないため DTCM に置く必要はない。
// FlashCart::new() で明示的にゼロ初期化するため MaybeUninit を使用。
#[unsafe(link_section = ".uninit.CART_RAM")]
static mut CART_RAM: core::mem::MaybeUninit<[u8; MAX_RAM]> =
    core::mem::MaybeUninit::uninit();

// ─────────────────────────────────────────────────────────────────────────────
// FlashCart: include_bytes! で Flash に埋め込まれた ROM
// ─────────────────────────────────────────────────────────────────────────────

/// カートリッジ種別 (ROM ヘッダ 0x147 から確定、以後は分岐に使うのみ)。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    RomOnly,
    Mbc1,
    Mbc3,
    Mbc5,
}

/// Flash に埋め込んだ ROM を CartridgeBus として提供する。
/// RomOnly / MBC1 / MBC3 (RTC はスタブ) / MBC5 をサポート。
pub struct FlashCart {
    rom: &'static [u8],
    kind: Kind,
    ram_size: usize,
    /// MBC5 は 9bit (0-511) まで使うため u16 で保持。MBC1/MBC3 はこの下位ビットのみ使う。
    rom_bank: u16,
    ram_bank: u8,
    ram_enabled: bool,
    /// MBC1 のみ: false=ROM banking mode, true=RAM banking mode
    mode: bool,
}

impl FlashCart {
    /// `rom` には `include_bytes!()` で取得した ROM スライスを渡す。
    pub fn new(rom: &'static [u8]) -> Self {
        // OCRAM (uninit セクション) を明示的にゼロ初期化する。
        // addr_of_mut! で static mut への参照を作らずポインタを取得する（Rust 2024 制約）。
        unsafe {
            core::ptr::write_bytes(
                core::ptr::addr_of_mut!(CART_RAM) as *mut u8,
                0,
                MAX_RAM,
            );
        }
        let cart_type = if rom.len() >= 0x148 { rom[0x147] } else { 0x00 };
        let kind = match cart_type {
            0x01..=0x03 => Kind::Mbc1,
            0x0F..=0x13 => Kind::Mbc3,
            0x19..=0x1E => Kind::Mbc5,
            _ => Kind::RomOnly,
        };
        let has_ram = matches!(
            cart_type,
            0x02 | 0x03 | 0x10 | 0x12 | 0x13 | 0x1A | 0x1B | 0x1D | 0x1E
        );
        let ram_size = if has_ram && rom.len() >= 0x14A {
            match rom[0x149] {
                0x01 => 0x800,   //  2KB
                0x02 => 0x2000,  //  8KB
                0x03 => 0x8000,  // 32KB
                0x04 => 0x20000, // 128KB (MAX_RAM 超過時は起動時に assert で検知)
                0x05 => 0x10000, // 64KB
                _ => 0,
            }
        } else {
            0
        };
        assert!(ram_size <= MAX_RAM, "cartridge RAM size exceeds MAX_RAM buffer");
        Self {
            rom,
            kind,
            ram_size,
            rom_bank: 1,
            ram_bank: 0,
            ram_enabled: false,
            mode: false,
        }
    }

    fn ram_read(&self, offset: usize) -> u8 {
        if offset < self.ram_size {
            unsafe { core::ptr::read((core::ptr::addr_of!(CART_RAM) as *const u8).add(offset)) }
        } else {
            0xFF
        }
    }

    fn ram_write(&mut self, offset: usize, val: u8) {
        if offset < self.ram_size {
            unsafe {
                core::ptr::write((core::ptr::addr_of_mut!(CART_RAM) as *mut u8).add(offset), val)
            }
        }
    }
}

impl CartridgeBus for FlashCart {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => {
                if self.kind == Kind::Mbc1 {
                    let bank = if self.mode && self.rom.len() > 0x80000 {
                        ((self.ram_bank as u16) << 5) & 0x60
                    } else {
                        0
                    };
                    let offset = bank as usize * 0x4000 + addr as usize;
                    self.rom.get(offset).copied().unwrap_or(0xFF)
                } else {
                    self.rom.get(addr as usize).copied().unwrap_or(0xFF)
                }
            }

            0x4000..=0x7FFF => {
                let bank = match self.kind {
                    Kind::Mbc1 => {
                        // 下位 5bit の 0→1 補正は上位ビット(0x60)と OR する前に行う必要がある
                        // (実機仕様: 0x00/0x20/0x40/0x60 書き込みはそれぞれ 1/0x21/0x41/0x61 選択)。
                        let mut low = self.rom_bank & 0x1F;
                        if low == 0 {
                            low = 1;
                        }
                        if self.rom.len() > 0x80000 {
                            low | (((self.ram_bank as u16) << 5) & 0x60)
                        } else {
                            low
                        }
                    }
                    Kind::Mbc3 => {
                        let bank = self.rom_bank & 0x7F;
                        if bank == 0 {
                            1
                        } else {
                            bank
                        }
                    }
                    // MBC5 のみバンク 0 も有効な値として扱う。
                    Kind::Mbc5 => self.rom_bank & 0x1FF,
                    Kind::RomOnly => return self.rom.get(addr as usize).copied().unwrap_or(0xFF),
                };
                let offset = bank as usize * 0x4000 + (addr as usize - 0x4000);
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }

            0xA000..=0xBFFF => match self.kind {
                Kind::Mbc1 => {
                    if !self.ram_enabled || self.ram_size == 0 {
                        0xFF
                    } else {
                        let bank = if self.mode { self.ram_bank } else { 0 };
                        self.ram_read(bank as usize * 0x2000 + (addr as usize - 0xA000))
                    }
                }
                Kind::Mbc3 => {
                    if !self.ram_enabled {
                        0xFF
                    } else {
                        match self.ram_bank {
                            0x00..=0x03 => {
                                self.ram_read(self.ram_bank as usize * 0x2000 + (addr as usize - 0xA000))
                            }
                            0x08..=0x0C => 0x00, // RTC レジスタ（スタブ）
                            _ => 0xFF,
                        }
                    }
                }
                Kind::Mbc5 => {
                    if !self.ram_enabled || self.ram_size == 0 {
                        0xFF
                    } else {
                        self.ram_read(self.ram_bank as usize * 0x2000 + (addr as usize - 0xA000))
                    }
                }
                Kind::RomOnly => 0xFF,
            },

            _ => 0xFF,
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        if self.kind == Kind::RomOnly {
            return;
        }
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = (val & 0x0F) == 0x0A,
            0x2000..=0x2FFF if self.kind == Kind::Mbc5 => {
                self.rom_bank = (self.rom_bank & 0x100) | val as u16;
            }
            0x3000..=0x3FFF if self.kind == Kind::Mbc5 => {
                self.rom_bank = (self.rom_bank & 0x0FF) | (((val & 0x01) as u16) << 8);
            }
            0x2000..=0x3FFF => {
                let mask = if self.kind == Kind::Mbc3 { 0x7F } else { 0x1F };
                self.rom_bank = (val & mask) as u16;
            }
            0x4000..=0x5FFF => {
                self.ram_bank = if self.kind == Kind::Mbc1 { val & 0x03 } else { val & 0x0F };
            }
            0x6000..=0x7FFF => {
                if self.kind == Kind::Mbc1 {
                    self.mode = (val & 0x01) != 0;
                }
                // MBC3: RTC ラッチ（未実装、no-op）
            }
            0xA000..=0xBFFF => match self.kind {
                Kind::Mbc1 => {
                    if self.ram_enabled && self.ram_size > 0 {
                        let bank = if self.mode { self.ram_bank } else { 0 };
                        self.ram_write(bank as usize * 0x2000 + (addr as usize - 0xA000), val);
                    }
                }
                Kind::Mbc3 => {
                    if self.ram_enabled && self.ram_bank <= 0x03 {
                        self.ram_write(
                            self.ram_bank as usize * 0x2000 + (addr as usize - 0xA000),
                            val,
                        );
                    }
                    // RTC レジスタ (0x08-0x0C) への書き込みはスタブのため無視
                }
                Kind::Mbc5 => {
                    if self.ram_enabled && self.ram_size > 0 {
                        self.ram_write(
                            self.ram_bank as usize * 0x2000 + (addr as usize - 0xA000),
                            val,
                        );
                    }
                }
                Kind::RomOnly => {}
            },
            _ => {}
        }
    }
}
