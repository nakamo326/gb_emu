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

struct SpriteData {
    x: u8,
    y: u8,
    tile_num: u8,
    flags: u8,
    /// OAM インデックス。同一 X 座標時の優先度（小さい方が前面）を保つための安定ソート鍵。
    order: u8,
}

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
    vram: [u8; 0x2000],
    oam: [u8; 0xA0],
    buffer: [u8; LCD_WIDTH * LCD_HEIGHT],
    /// BGP 適用前のピクセル値（スプライト優先度判定用）
    bg_pixel_buffer: [u8; LCD_WIDTH * LCD_HEIGHT],
    /// OAMScan で収集したスプライト（最大10）
    sprite_buffer: heapless::Vec<SpriteData, 10>,
    /// ウィンドウ内部 Y カウンタ（VBlank でリセット）
    window_line_counter: u8,
    cycle: u8,
    /// VBlank 割り込み要求フラグ
    pub vblank_irq: bool,
    /// STAT 割り込み要求フラグ
    pub stat_irq: bool,
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
            vram: [0; 0x2000],
            oam: [0; 0xA0],
            buffer: [0; LCD_WIDTH * LCD_HEIGHT],
            bg_pixel_buffer: [0; LCD_WIDTH * LCD_HEIGHT],
            sprite_buffer: heapless::Vec::new(),
            window_line_counter: 0,
            vblank_irq: false,
            stat_irq: false,
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
            _ => 0xFF,
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
            0xFF42 => self.scy = val,
            0xFF43 => self.scx = val,
            0xFF44 => {}
            0xFF45 => self.lyc = val,
            0xFF47 => self.bgp = val,
            0xFF48 => self.obp0 = val,
            0xFF49 => self.obp1 = val,
            0xFF46 => {} // DMA転送はMMU側で処理済み
            0xFF4A => self.wy = val,
            0xFF4B => self.wx = val,
            _ => {}
        }
    }

    fn get_pixel_from_tile(&self, tile_idx: usize, row: u8, col: u8) -> u8 {
        // 8x8タイルの1ピクセルを取得する
        // 一行2byte
        let r = (row * 2) as usize;
        //
        let c = (7 - col) as usize;

        // 0x8000からのオフセットを計算
        let tile_addr = tile_idx << 4;

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
            // 0x8800アドレッシングモード: タイル番号は符号付き、ベースは0x9000 (VRAM offset 0x1000)
            // tile_idx = 0x100 + (signed_byte) なので VRAM offset = tile_idx * 16 = 0x1000 + signed * 16
            (0x100i16 + (ret as i8) as i16) as usize
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

            let pixel_row = y & 7;
            let pixel_col = x & 7;

            let pixel = self.get_pixel_from_tile(tile_idx, pixel_row, pixel_col);
            let buf_idx = LCD_WIDTH * self.ly as usize + i;
            self.bg_pixel_buffer[buf_idx] = pixel;

            let palette_idx = (self.bgp >> (pixel << 1)) & 0b11;
            self.buffer[buf_idx] = palette_idx;
        }
    }

    fn collect_sprites(&mut self) {
        self.sprite_buffer.clear();
        let sprite_height: u8 = if self.lcdc & SPRITE_SIZE != 0 { 16 } else { 8 };
        for i in 0..40usize {
            if self.sprite_buffer.len() >= 10 {
                break;
            }
            let base = i * 4;
            let y = self.oam[base];
            let x = self.oam[base + 1];
            let tile_num = self.oam[base + 2];
            let flags = self.oam[base + 3];
            // スプライトの画面 Y 座標 (OAM の y は +16 オフセット)
            let screen_y = y.wrapping_sub(16);
            if self.ly >= screen_y && self.ly < screen_y.wrapping_add(sprite_height) {
                let _ = self.sprite_buffer.push(SpriteData { x, y, tile_num, flags, order: i as u8 });
            }
        }
        // X 座標で安定ソート（小さい X が優先、同値は OAM 順）。
        // heapless::Vec は alloc 依存の sort_by_key を持たないため slice の
        // sort_unstable_by を使い、order を第二鍵にして安定性を確保する。
        self.sprite_buffer
            .sort_unstable_by(|a, b| a.x.cmp(&b.x).then(a.order.cmp(&b.order)));
    }

    fn render_sprites(&mut self) {
        if self.lcdc & SPRITE_ENABLE == 0 {
            return;
        }
        let sprite_height: u8 = if self.lcdc & SPRITE_SIZE != 0 { 16 } else { 8 };
        // 逆順で描画（X が小さいスプライトが最終的に上書きして前面に来る）
        for i in (0..self.sprite_buffer.len()).rev() {
            let s = &self.sprite_buffer[i];
            let screen_x = s.x.wrapping_sub(8);
            let screen_y = s.y.wrapping_sub(16);
            let palette = if s.flags & 0x10 != 0 { self.obp1 } else { self.obp0 };
            let y_flip = s.flags & 0x40 != 0;
            let x_flip = s.flags & 0x20 != 0;
            let bg_priority = s.flags & 0x80 != 0;

            let row = self.ly.wrapping_sub(screen_y);
            let tile_row = if y_flip { sprite_height - 1 - row } else { row };

            // 8x16 モード: 上半分は tile_num の bit0 をクリア、下半分はセット
            let tile_num = if sprite_height == 16 {
                if tile_row < 8 { s.tile_num & 0xFE } else { s.tile_num | 0x01 }
            } else {
                s.tile_num
            };
            let effective_row = tile_row & 7;

            for col in 0u8..8 {
                let px = screen_x.wrapping_add(col);
                if px >= LCD_WIDTH as u8 {
                    continue;
                }
                let tile_col = if x_flip { 7 - col } else { col };
                let pixel = self.get_pixel_from_tile(tile_num as usize, effective_row, tile_col);
                if pixel == 0 {
                    continue; // カラー 0 = 透明
                }
                let buf_idx = LCD_WIDTH * self.ly as usize + px as usize;
                // bg_priority=1 の場合、BG カラーが 0 でなければスプライトを隠す
                if bg_priority && self.bg_pixel_buffer[buf_idx] != 0 {
                    continue;
                }
                let palette_idx = (palette >> (pixel << 1)) & 0b11;
                self.buffer[buf_idx] = palette_idx;
            }
        }
    }

    fn render_window(&mut self) {
        if self.lcdc & WINDOW_ENABLE == 0 || self.lcdc & BG_WINDOW_ENABLE == 0 {
            return;
        }
        if self.ly < self.wy {
            return;
        }
        let win_x_start = self.wx.saturating_sub(7) as usize;
        if win_x_start >= LCD_WIDTH {
            return;
        }
        let tile_map = self.lcdc & WINDOW_TILE_MAP != 0;
        for i in win_x_start..LCD_WIDTH {
            let x = (i - win_x_start) as u8;
            let tile_row = self.window_line_counter / 8;
            let tile_col = x / 8;
            let tile_idx = self.get_tile_idx_from_tile_map(tile_map, tile_row, tile_col);
            let pixel_row = self.window_line_counter & 7;
            let pixel_col = x & 7;
            let pixel = self.get_pixel_from_tile(tile_idx, pixel_row, pixel_col);
            let buf_idx = LCD_WIDTH * self.ly as usize + i;
            self.bg_pixel_buffer[buf_idx] = pixel;
            let palette_idx = (self.bgp >> (pixel << 1)) & 0b11;
            self.buffer[buf_idx] = palette_idx;
        }
        self.window_line_counter += 1;
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
                    if self.stat & OAM_SCAN_INT != 0 {
                        self.stat_irq = true;
                    }
                } else {
                    self.mode = Mode::VBlank;
                    self.cycle = 114;
                    self.window_line_counter = 0;
                    self.vblank_irq = true;
                    if self.stat & VBLANK_INT != 0 {
                        self.stat_irq = true;
                    }
                }
                self.check_lyc_eq_ly();
                if self.ly == self.lyc && self.stat & LYC_EQ_LY_INT != 0 {
                    self.stat_irq = true;
                }
            }
            Mode::VBlank => {
                self.ly += 1;
                if self.ly > 153 {
                    self.ly = 0;
                    self.mode = Mode::OAMScan;
                    self.cycle = 20;
                    is_vsync = true;
                    if self.stat & OAM_SCAN_INT != 0 {
                        self.stat_irq = true;
                    }
                } else {
                    self.cycle = 114;
                }
                self.check_lyc_eq_ly();
                if self.ly == self.lyc && self.stat & LYC_EQ_LY_INT != 0 {
                    self.stat_irq = true;
                }
            }
            Mode::OAMScan => {
                self.collect_sprites();
                self.mode = Mode::Drawing;
                self.cycle = 43;
            }
            Mode::Drawing => {
                self.render_bg();
                self.render_window();
                self.render_sprites();
                self.mode = Mode::HBlank;
                self.cycle = 51;
                if self.stat & HBLANK_INT != 0 {
                    self.stat_irq = true;
                }
            }
        }

        is_vsync
    }

    pub fn pixel_buffer(&self) -> &[u8] {
        &self.buffer
    }
}
