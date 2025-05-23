#[derive(Copy, Clone, PartialEq, Eq)]
enum Mode {
    HBlank,
    VBlank,
    OAMScan,
    Drawing,
}

const PPU_ENABLE: u8 = 1 << 7;
const WINDOW_TILE_MAP: u8 = 1 << 6;
const WINDOW_ENABLE: u8 = 1 << 5;
const TILE_DATA_ADDRESSING_MODE: u8 = 1 << 4;
const BG_TILE_MAP: u8 = 1 << 3;
const SPRITE_SIZE: u8 = 1 << 2;
const SPRITE_ENABLE: u8 = 1 << 1;
const BG_WINDOW_ENABLE: u8 = 1 << 0;

const LYC_EQ_LY_INT: u8 = 1 << 6;
const OAM_SCAN_INT: u8 = 1 << 5;
const VBLANK_INT: u8 = 1 << 4;
const HBLANK_INT: u8 = 1 << 3;
const LYC_EQ_LY: u8 = 1 << 2;

pub struct Ppu {
    mode: Mode,
    lcdc: u8,
    stat: u8,
    scy: u8,
    scx: u8,
    ly: u8,
    lyc: u8,
    bgp: u8,
    obp0: u8,
    obp1: u8,
    wy: u8,
    wx: u8,
    vram: Box<[u8; 0x2000]>,
    oam: Box<[u8; 0xA0]>,
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            mode: Mode::OAMScan,
            lcdc: 0,
            stat: 0,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0,
            obp0: 0,
            obp1: 0,
            wy: 0,
            wx: 0,
            vram: Box::new([0; 0x2000]),
            oam: Box::new([0; 0xA0]),
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => {
                if self.mode == Mode::Drawing {
                    0xFF
                } else {
                    self.vram[addr as usize & 0x1FFF]
                }
            }
            0xFE00..=0xFE9F => {
                if self.mode == Mode::Drawing || self.mode == Mode::OAMScan {
                    0xFF
                } else {
                    self.oam[addr as usize & 0xFF]
                }
            }
            0xFF40 => self.lcdc,
            0xFF41 => 0x80 | self.stat | self.mode as u8, // 最上位bitは常に1、下の二桁はmode
            0xFF42 => self.scy,
            0xFF43 => self.scx,
            0xFF44 => self.ly,
            0xFF45 => self.lyc,
            0xFF47 => self.bgp,
            0xFF48 => self.obp0,
            0xFF49 => self.obp1,
            0xFF4A => self.wy,
            0xFF4B => self.wx,
            _ => unreachable!(),
        }
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x8000..=0x9FFF => {
                if self.mode != Mode::Drawing {
                    self.vram[addr as usize & 0x1FFF] = val;
                }
            }
            0xFE00..=0xFE9F => {
                if self.mode != Mode::Drawing && self.mode != Mode::OAMScan {
                    self.oam[addr as usize & 0xFF] = val;
                }
            }
            0xFF40 => self.lcdc = val,
            0xFF41 => self.stat = (self.stat & LYC_EQ_LY) | (val & 0xF8),
            0xFF42 => {}
            0xFF43 => {}
            0xFF44 => {}
            0xFF45 => {}
            0xFF47 => {}
            0xFF48 => {}
            0xFF49 => {}
            0xFF4A => {}
            0xFF4B => {}
            _ => unreachable!(),
        }
    }

    fn get_pixel_from_tile(&self, tile_idx: usize, row: u8, col: u8) -> u8 {
        // 8x8タイルの1ピクセルを取得する
        // 一行2byte
        let r = (row * 2) as usize;
        //
        let c = (7 - col) as usize;

        // 0x8000からのオフセットを計算
        let tile_addr = tile_idx * 16;

        // 0x1FFFはVRAMのアドレス範囲
        let low = self.vram[(tile_addr | r) & 0x1FFF];
        let high = self.vram[(tile_addr | (r + 1)) & 0x1FFF];

        let pixel = ((low >> c) & 1) | (((high >> c) & 1) << 1);
        pixel
    }
}
