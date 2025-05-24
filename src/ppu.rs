use std::iter;

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

pub const LCD_WIDTH: usize = 160;
pub const LCD_HEIGHT: usize = 144;

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
    buffer: Box<[u8; LCD_WIDTH * LCD_HEIGHT * 4]>,
    cycle: u8,
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
            cycle: 20,
            vram: Box::new([0; 0x2000]),
            oam: Box::new([0; 0xA0]),
            buffer: Box::new([0; LCD_WIDTH * LCD_HEIGHT * 4]),
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

    fn get_tile_idx_from_tile_map(&self, tile_map: bool, row: u8, col: u8) -> usize {
        // tile_mapは２つある FIXME: bool???
        let tile_map_addr = if tile_map { 0x1C00 } else { 0x1800 };

        let ret = self.vram[tile_map_addr | (((row as usize) << 5) + col as usize)];

        if self.lcdc & TILE_DATA_ADDRESSING_MODE > 0 {
            ret as usize
        } else {
            ((ret as i8) as i16 + 128) as usize
        }
    }

    fn render_bg(&mut self) {
        if self.lcdc & BG_WINDOW_ENABLE == 0 {
            return;
        }
        let y = self.ly.wrapping_add(self.scy);
        for i in 0..LCD_WIDTH {
            let x = (i as u8).wrapping_add(self.scx);

            let tile_row = y / 8;
            let tile_col = x / 8;
            let tile_map = self.lcdc & BG_TILE_MAP > 0;

            let tile_idx =
                self.get_tile_idx_from_tile_map(tile_map, tile_row as u8, tile_col as u8);

            //
            let pixel_row = y & 7;
            let pixel_col = x & 7;

            let pixel = self.get_pixel_from_tile(tile_idx, pixel_row, pixel_col);

            self.buffer[LCD_WIDTH * self.ly as usize + i] = match (self.bgp >> (pixel << 1)) & 0b11
            {
                0b00 => 0xFF,
                0b01 => 0xAA,
                0b10 => 0x55,
                _ => 0x00,
            };
        }
    }

    fn check_lyc_eq_ly(&mut self) {
        if self.ly == self.lyc {
            self.stat |= LYC_EQ_LY;
        } else {
            self.stat &= !LYC_EQ_LY;
        }
    }

    pub fn emulate_cycle(&mut self) -> bool {
        if self.lcdc & PPU_ENABLE == 0 {
            return false;
        }

        self.cycle -= 1;
        if self.cycle > 0 {
            return false;
        }

        // vsyncであるかを示す変数
        let mut is_vsync = false;

        match self.mode {
            Mode::HBlank => {
                self.ly += 1;
                if self.ly < 144 {
                    self.mode = Mode::OAMScan;
                    self.cycle = 20;
                } else {
                    self.mode = Mode::VBlank;
                    self.cycle = 114;
                }
                self.check_lyc_eq_ly();
            }
            Mode::VBlank => {
                self.ly += 1;
                if self.ly > 153 {
                    self.ly = 0;
                    self.mode = Mode::OAMScan;
                    self.cycle = 20;
                    is_vsync = true;
                } else {
                    self.cycle = 114;
                }
                self.check_lyc_eq_ly();
            }
            Mode::OAMScan => {
                self.mode = Mode::Drawing;
                self.cycle = 43;
            }
            Mode::Drawing => {
                self.render_bg();
                self.mode = Mode::HBlank;
                self.cycle = 51;
            }
        }

        is_vsync
    }

    pub fn pixel_buffer(&self) -> Box<[u8]> {
        self.buffer
            .iter()
            .flat_map(|&e| iter::repeat(e.into()).take(3))
            .collect::<Box<[u8]>>()
    }
}
