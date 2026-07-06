//! GBA メモリバス。アドレス上位 8 ビットで各領域へディスパッチする。
//!
//! | アドレス | 領域 |
//! |---|---|
//! | 0x00000000-0x00003FFF | BIOS (16KB) |
//! | 0x02000000- | EWRAM 256KB (ミラー) |
//! | 0x03000000- | IWRAM 32KB (ミラー) |
//! | 0x04000000- | I/O レジスタ |
//! | 0x05000000- | パレット RAM 1KB (ミラー) |
//! | 0x06000000- | VRAM 96KB (ミラー) |
//! | 0x07000000- | OAM 1KB (ミラー) |
//! | 0x08000000-0x0DFFFFFF | ROM (最大 32MB) |
//! | 0x0E000000- | SRAM 64KB (8bit バス) |

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use crate::dma::Dma;
use crate::ppu::Ppu;
use crate::timer::Timers;

pub const BIOS_SIZE: usize = 0x4000;
const EWRAM_SIZE: usize = 0x4_0000;
const IWRAM_SIZE: usize = 0x8000;
const SRAM_SIZE: usize = 0x1_0000;

pub struct Bus {
    pub bios: Option<Vec<u8>>,
    ewram: Box<[u8]>,
    iwram: Box<[u8]>,
    pub rom: Vec<u8>,
    pub sram: Box<[u8]>,
    /// SRAM に書き込みがあった（ホストのセーブファイル永続化判定用）
    pub sram_dirty: bool,

    pub ppu: Ppu,
    pub dma: Dma,
    pub timers: Timers,

    /// 押下中キーのビットマスク（1=押下）。KEYINPUT はこの反転を返す
    keys: u16,
    keycnt: u16,
    pub ie: u16,
    pub if_: u16,
    pub ime: bool,
    waitcnt: u16,
    postflg: u8,
    /// HALTCNT 書き込みで立つ。CPU 側が halt へ移行して消費する
    pub halt_request: bool,
    /// APU 未実装のためサウンドレジスタ (0x60-0xAF) は RAM バッキングで代用
    sound_regs: [u8; 0x50],
}

impl Bus {
    pub fn new(rom: Vec<u8>, bios: Option<Vec<u8>>) -> Self {
        Self {
            bios,
            ewram: vec![0; EWRAM_SIZE].into_boxed_slice(),
            iwram: vec![0; IWRAM_SIZE].into_boxed_slice(),
            rom,
            sram: vec![0xFF; SRAM_SIZE].into_boxed_slice(),
            sram_dirty: false,
            ppu: Ppu::new(),
            dma: Dma::new(),
            timers: Timers::new(),
            keys: 0,
            keycnt: 0,
            ie: 0,
            if_: 0,
            ime: false,
            waitcnt: 0,
            postflg: 0,
            halt_request: false,
            sound_regs: [0; 0x50],
        }
    }

    /// HLE BIOS の RegisterRamReset 用アクセサ
    pub(crate) fn ewram_mut(&mut self) -> &mut [u8] {
        &mut self.ewram
    }

    pub(crate) fn iwram_mut(&mut self) -> &mut [u8] {
        &mut self.iwram
    }

    /// キー状態を更新し、KEYCNT の条件を満たせば割り込みを要求する。
    pub fn set_keys(&mut self, keys: u16) {
        self.keys = keys & 0x3FF;
        if self.keycnt & 0x4000 != 0 {
            let mask = self.keycnt & 0x3FF;
            let hit = if self.keycnt & 0x8000 != 0 {
                self.keys & mask == mask && mask != 0 // AND: 全キー同時押し
            } else {
                self.keys & mask != 0 // OR: いずれか押下
            };
            if hit {
                self.if_ |= 0x1000;
            }
        }
    }

    pub fn read8(&mut self, addr: u32) -> u8 {
        match addr >> 24 {
            0x0 => {
                let a = addr as usize;
                match &self.bios {
                    Some(b) if a < BIOS_SIZE => b[a],
                    _ => 0,
                }
            }
            0x2 => self.ewram[addr as usize & (EWRAM_SIZE - 1)],
            0x3 => self.iwram[addr as usize & (IWRAM_SIZE - 1)],
            0x4 => {
                let half = self.io_read16(addr & !1);
                (half >> ((addr & 1) * 8)) as u8
            }
            0x5 => self.ppu.palette[addr as usize & 0x3FF],
            0x6 => self.ppu.vram[vram_index(addr)],
            0x7 => self.ppu.oam[addr as usize & 0x3FF],
            0x8..=0xD => {
                let a = addr as usize & 0x1FF_FFFF;
                if a < self.rom.len() {
                    self.rom[a]
                } else {
                    // ROM 範囲外はオープンバス: アドレスの半語値が読める
                    ((addr >> 1) >> ((addr & 1) * 8)) as u8
                }
            }
            0xE | 0xF => self.sram[addr as usize & (SRAM_SIZE - 1)],
            _ => 0,
        }
    }

    pub fn read16(&mut self, addr: u32) -> u16 {
        let addr = addr & !1;
        if addr >> 24 == 0x4 {
            return self.io_read16(addr);
        }
        self.read8(addr) as u16 | (self.read8(addr + 1) as u16) << 8
    }

    pub fn read32(&mut self, addr: u32) -> u32 {
        let addr = addr & !3;
        self.read16(addr) as u32 | (self.read16(addr + 2) as u32) << 16
    }

    pub fn write8(&mut self, addr: u32, val: u8) {
        match addr >> 24 {
            0x2 => self.ewram[addr as usize & (EWRAM_SIZE - 1)] = val,
            0x3 => self.iwram[addr as usize & (IWRAM_SIZE - 1)] = val,
            0x4 => {
                let shift = (addr & 1) * 8;
                self.io_write16(addr & !1, (val as u16) << shift, 0xFF << shift);
            }
            // パレット/VRAM へのバイト書き込みは半語の両バイトに複製される
            0x5 => {
                let a = addr as usize & 0x3FE;
                self.ppu.palette[a] = val;
                self.ppu.palette[a + 1] = val;
            }
            0x6 => {
                // OBJ タイル領域へのバイト書き込みは無視される（BG 領域のみ複製書き込み）
                let a = vram_index(addr & !1);
                let obj_start = if self.ppu.bitmap_mode() { 0x14000 } else { 0x10000 };
                if a < obj_start {
                    self.ppu.vram[a] = val;
                    self.ppu.vram[a + 1] = val;
                }
            }
            // OAM へのバイト書き込みは無視される
            0x7 => {}
            0xE | 0xF => {
                self.sram[addr as usize & (SRAM_SIZE - 1)] = val;
                self.sram_dirty = true;
            }
            _ => {}
        }
    }

    pub fn write16(&mut self, addr: u32, val: u16) {
        let addr = addr & !1;
        match addr >> 24 {
            0x4 => self.io_write16(addr, val, 0xFFFF),
            0x5 => {
                let a = addr as usize & 0x3FE;
                self.ppu.palette[a] = val as u8;
                self.ppu.palette[a + 1] = (val >> 8) as u8;
            }
            0x6 => {
                let a = vram_index(addr);
                self.ppu.vram[a] = val as u8;
                self.ppu.vram[a + 1] = (val >> 8) as u8;
            }
            0x7 => {
                let a = addr as usize & 0x3FE;
                self.ppu.oam[a] = val as u8;
                self.ppu.oam[a + 1] = (val >> 8) as u8;
            }
            _ => {
                self.write8(addr, val as u8);
                self.write8(addr + 1, (val >> 8) as u8);
            }
        }
    }

    pub fn write32(&mut self, addr: u32, val: u32) {
        let addr = addr & !3;
        self.write16(addr, val as u16);
        self.write16(addr + 2, (val >> 16) as u16);
    }

    fn io_read16(&mut self, addr: u32) -> u16 {
        let off = (addr & 0x3FF) as usize;
        match off {
            0x000..=0x056 => self.ppu.read_io(off),
            0x060..=0x0AE => {
                let i = off - 0x60;
                self.sound_regs[i] as u16 | (self.sound_regs[i + 1] as u16) << 8
            }
            0x0B0..=0x0DE => self.dma.read_io(off),
            0x100..=0x10E => self.timers.read_io(off),
            0x130 => !self.keys & 0x3FF,
            0x132 => self.keycnt,
            0x200 => self.ie,
            0x202 => self.if_,
            0x204 => self.waitcnt,
            0x208 => self.ime as u16,
            0x300 => self.postflg as u16,
            _ => 0,
        }
    }

    /// I/O レジスタは 16bit 単位が基本のため、バイトアクセスは mask で表現する。
    /// （IF への ack がバイト書き込みでも隣のバイトに波及しないようにするため）
    fn io_write16(&mut self, addr: u32, val: u16, mask: u16) {
        let off = (addr & 0x3FF) as usize;
        match off {
            0x000..=0x056 => self.ppu.write_io(off, val, mask),
            0x060..=0x0AE => {
                let i = off - 0x60;
                if mask & 0x00FF != 0 {
                    self.sound_regs[i] = val as u8;
                }
                if mask & 0xFF00 != 0 {
                    self.sound_regs[i + 1] = (val >> 8) as u8;
                }
            }
            0x0B0..=0x0DE => self.dma.write_io(off, val, mask),
            0x100..=0x10E => self.timers.write_io(off, val, mask),
            0x132 => self.keycnt = self.keycnt & !mask | val & mask,
            0x200 => self.ie = (self.ie & !mask | val & mask) & 0x3FFF,
            // IF は書いたビットをクリア（acknowledge）
            0x202 => self.if_ &= !(val & mask),
            0x204 => self.waitcnt = self.waitcnt & !mask | val & mask,
            0x208 => {
                if mask & 0x00FF != 0 {
                    self.ime = val & 1 != 0;
                }
            }
            0x300 => {
                if mask & 0x00FF != 0 {
                    self.postflg = val as u8 & 1;
                }
                // HALTCNT (0x301): bit7=0 Halt / 1 Stop。Stop も Halt として扱う
                if mask & 0xFF00 != 0 {
                    self.halt_request = true;
                }
            }
            _ => {}
        }
    }

    /// vblank/hblank イベントで該当タイミングの DMA チャンネルを起動する。
    pub fn dma_trigger(&mut self, timing: u16) {
        self.dma.trigger(timing);
    }

    /// pending になっている DMA を優先度順にすべて実行する。
    pub fn dma_service(&mut self) {
        // 転送中のバスアクセスと DMA レジスタの二重借用を避けるため、
        // チャンネル状態をコピーして実行し、結果を書き戻す
        while let Some(idx) = self.dma.next_pending() {
            let mut ch = self.dma.ch[idx];
            ch.pending = false;
            let ctrl = ch.cnt;
            let unit: u32 = if ctrl & 0x0400 != 0 { 4 } else { 2 };
            let dst_step = step_of((ctrl >> 5) & 3, unit);
            let src_step = step_of((ctrl >> 7) & 3, unit);
            let mut src = ch.src & !(unit - 1);
            let mut dst = ch.dst & !(unit - 1);
            for _ in 0..ch.remaining {
                if unit == 4 {
                    let v = self.read32(src);
                    self.write32(dst, v);
                } else {
                    let v = self.read16(src);
                    self.write16(dst, v);
                }
                src = src.wrapping_add_signed(src_step);
                dst = dst.wrapping_add_signed(dst_step);
            }
            ch.src = src;
            ch.dst = dst;
            if ctrl & 0x4000 != 0 {
                self.if_ |= 0x100 << idx;
            }
            let timing = (ctrl >> 12) & 3;
            if ctrl & 0x0200 != 0 && timing != 0 {
                // リピート: カウントを再ロードし、dst モード 3 なら宛先も再ロード
                ch.remaining = Dma::count_of(idx, ch.count);
                if (ctrl >> 5) & 3 == 3 {
                    ch.dst = ch.dad;
                }
            } else {
                ch.cnt &= !0x8000;
            }
            self.dma.ch[idx] = ch;
        }
    }
}

/// DMA のアドレス増分 (0:inc 1:dec 2:fixed 3:inc-reload)
fn step_of(mode: u16, unit: u32) -> i32 {
    match mode {
        1 => -(unit as i32),
        2 => 0,
        _ => unit as i32,
    }
}

/// VRAM は 128KB 空間に 96KB が「64K+32K+32K(後半 32K は前半 32K のミラー)」で載る
fn vram_index(addr: u32) -> usize {
    let a = addr as usize & 0x1_FFFF;
    if a >= 0x1_8000 { a - 0x8000 } else { a }
}
