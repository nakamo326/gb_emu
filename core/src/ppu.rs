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

/// RGB555 形式: bits 0-4=R, bits 5-9=G, bits 10-14=B（GBC ネイティブ形式）
const fn rgb555(r: u8, g: u8, b: u8) -> u16 {
    (r as u16 >> 3) | ((g as u16 >> 3) << 5) | ((b as u16 >> 3) << 10)
}

/// DMG グリーン 4 色パレット（RGB555）
const DMG_PALETTE: [u16; 4] = [
    rgb555(0xE0, 0xF8, 0xD0),
    rgb555(0x88, 0xC0, 0x70),
    rgb555(0x34, 0x68, 0x56),
    rgb555(0x0E, 0x18, 0x20),
];

struct SpriteData {
    x: u8,
    y: u8,
    tile_num: u8,
    flags: u8,
    /// OAM インデックス。同一 X 座標時の優先度（小さい方が前面）を保つための安定ソート鍵。
    order: u8,
}

pub struct Ppu {
    /// CGB モードで動作しているか（ROM ヘッダ 0x0143 で決定）
    pub cgb_mode: bool,
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
    /// VRAM: バンク 0 (0x8000-0x9FFF) + バンク 1 (CGB 専用)
    vram: [[u8; 0x2000]; 2],
    /// VBK (0xFF4F): 現在の VRAM バンク番号 (0 or 1)
    vbk: u8,
    oam: [u8; 0xA0],
    buffer: [u16; LCD_WIDTH * LCD_HEIGHT],
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
    /// CGB BG カラーパレット RAM (8 パレット × 4 色 × 2 バイト = 64 バイト)
    bg_palette_ram: [u8; 64],
    /// BCPS (0xFF68): BG パレット仕様レジスタ（インデックス + オートインクリメント）
    bcps: u8,
    /// CGB OBJ カラーパレット RAM (8 パレット × 4 色 × 2 バイト = 64 バイト)
    obj_palette_ram: [u8; 64],
    /// OCPS (0xFF6A): OBJ パレット仕様レジスタ
    ocps: u8,
    /// OPRI (0xFF6C): OBJ 優先度モード (0=OAM 順, 1=X 座標順/DMG 互換)
    opri: u8,
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            cgb_mode: false,
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
            vram: [[0u8; 0x2000]; 2],
            vbk: 0,
            oam: [0; 0xA0],
            buffer: [0; LCD_WIDTH * LCD_HEIGHT],
            bg_pixel_buffer: [0; LCD_WIDTH * LCD_HEIGHT],
            sprite_buffer: heapless::Vec::new(),
            window_line_counter: 0,
            vblank_irq: false,
            stat_irq: false,
            bg_palette_ram: [0xFF; 64],
            bcps: 0,
            obj_palette_ram: [0xFF; 64],
            ocps: 0,
            opri: 0,
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => {
                if self.mode == Mode::Drawing {
                    0xFF
                } else {
                    self.vram[self.vbk as usize][addr as usize & 0x1FFF]
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
            0xFF41 => 0x80 | self.stat | self.mode as u8,
            0xFF42 => self.scy,
            0xFF43 => self.scx,
            0xFF44 => self.ly,
            0xFF45 => self.lyc,
            0xFF47 => self.bgp,
            0xFF48 => self.obp0,
            0xFF49 => self.obp1,
            0xFF4A => self.wy,
            0xFF4B => self.wx,
            // CGB レジスタ（DMG モードでは 0xFF を返す）
            0xFF4F => self.vbk | 0xFE, // 上位 7bit は 1 固定
            0xFF68 => self.bcps | if self.cgb_mode { 0 } else { 0xFF },
            0xFF69 => {
                if self.cgb_mode {
                    self.bg_palette_ram[(self.bcps & 0x3F) as usize]
                } else {
                    0xFF
                }
            }
            0xFF6A => self.ocps | if self.cgb_mode { 0 } else { 0xFF },
            0xFF6B => {
                if self.cgb_mode {
                    self.obj_palette_ram[(self.ocps & 0x3F) as usize]
                } else {
                    0xFF
                }
            }
            0xFF6C => self.opri | 0xFE,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x8000..=0x9FFF => {
                if self.mode != Mode::Drawing {
                    self.vram[self.vbk as usize][addr as usize & 0x1FFF] = val;
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
            // CGB レジスタ
            0xFF4F => {
                if self.cgb_mode {
                    self.vbk = val & 0x01;
                }
            }
            0xFF68 => {
                if self.cgb_mode {
                    self.bcps = val & 0xBF; // bit6 は書き込み可（オートインクリメント）
                }
            }
            0xFF69 => {
                if self.cgb_mode {
                    let idx = (self.bcps & 0x3F) as usize;
                    self.bg_palette_ram[idx] = val;
                    if self.bcps & 0x80 != 0 {
                        self.bcps = (self.bcps & 0x80) | ((idx as u8 + 1) & 0x3F);
                    }
                }
            }
            0xFF6A => {
                if self.cgb_mode {
                    self.ocps = val & 0xBF;
                }
            }
            0xFF6B => {
                if self.cgb_mode {
                    let idx = (self.ocps & 0x3F) as usize;
                    self.obj_palette_ram[idx] = val;
                    if self.ocps & 0x80 != 0 {
                        self.ocps = (self.ocps & 0x80) | ((idx as u8 + 1) & 0x3F);
                    }
                }
            }
            0xFF6C => {
                if self.cgb_mode {
                    self.opri = val & 0x01;
                }
            }
            _ => {}
        }
    }

    /// タイルデータ 1 ピクセルを取得する。`vram_bank` で参照バンクを指定（Phase 3 で CGB 対応）。
    fn get_pixel_from_tile(&self, tile_idx: usize, row: u8, col: u8, vram_bank: usize) -> u8 {
        // GB タイルは 2bpp: 1 行 = 2 バイト、low/high の同ビットを組んで 1 ピクセル(0-3)。
        // bit7 が左端なので列 col のビット位置は 7-col。
        let r = (row * 2) as usize;
        let c = (7 - col) as usize;
        let tile_addr = tile_idx << 4;

        let low = self.vram[vram_bank][(tile_addr | r) & 0x1FFF];
        let high = self.vram[vram_bank][(tile_addr | (r + 1)) & 0x1FFF];

        ((low >> c) & 1) | (((high >> c) & 1) << 1)
    }


    fn render_bg(&mut self) {
        // CGB: LCDC bit0=0 は "BG/Window master disable" → 画面を白で塗り透明扱い
        if self.lcdc & BG_WINDOW_ENABLE == 0 {
            if self.cgb_mode {
                let base = LCD_WIDTH * self.ly as usize;
                for i in 0..LCD_WIDTH {
                    self.bg_pixel_buffer[base + i] = 0;
                    self.buffer[base + i] = 0x7FFF;
                }
            }
            return;
        }
        let y = self.ly.wrapping_add(self.scy);
        let tile_map = self.lcdc & BG_TILE_MAP > 0;
        let tile_map_base = if tile_map { 0x1C00 } else { 0x1800 };

        for i in 0..LCD_WIDTH {
            let x = (i as u8).wrapping_add(self.scx);
            let tile_row = (y / 8) as usize;
            let tile_col = (x / 8) as usize;
            let tile_offset = (tile_row << 5) + tile_col;
            let tile_addr = tile_map_base + tile_offset;

            let raw_idx = self.vram[0][tile_addr];
            let tile_idx = if self.lcdc & TILE_DATA_ADDRESSING_MODE > 0 {
                raw_idx as usize
            } else {
                (0x100i16 + (raw_idx as i8) as i16) as usize
            };

            let pixel_row = y & 7;
            let pixel_col = x & 7;
            let buf_idx = LCD_WIDTH * self.ly as usize + i;

            if self.cgb_mode {
                let attrs = self.vram[1][tile_addr];
                let palette_num = (attrs & 0x07) as usize;
                let vram_bank = ((attrs >> 3) & 0x01) as usize;
                let h_flip = attrs & 0x20 != 0;
                let v_flip = attrs & 0x40 != 0;
                let bg_prio = attrs & 0x80 != 0;

                let r = if v_flip { 7 - pixel_row } else { pixel_row };
                let c = if h_flip { 7 - pixel_col } else { pixel_col };
                let pixel = self.get_pixel_from_tile(tile_idx, r, c, vram_bank);

                // bit7 に BG タイル優先度を格納（Phase 4 のスプライト優先度判定で使用）
                self.bg_pixel_buffer[buf_idx] = pixel | if bg_prio { 0x80 } else { 0 };

                let palette_base = palette_num * 8 + pixel as usize * 2;
                let lo = self.bg_palette_ram[palette_base];
                let hi = self.bg_palette_ram[palette_base + 1];
                self.buffer[buf_idx] = lo as u16 | ((hi as u16) << 8);
            } else {
                let pixel = self.get_pixel_from_tile(tile_idx, pixel_row, pixel_col, 0);
                self.bg_pixel_buffer[buf_idx] = pixel;
                let color_idx = (self.bgp >> (pixel << 1)) & 0b11;
                self.buffer[buf_idx] = DMG_PALETTE[color_idx as usize];
            }
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
                let pixel = self.get_pixel_from_tile(tile_num as usize, effective_row, tile_col, 0);
                if pixel == 0 {
                    continue; // カラー 0 = 透明
                }
                let buf_idx = LCD_WIDTH * self.ly as usize + px as usize;
                // OAM bit7=1 かつ BG ピクセル(bits 0-1)が非透明ならスプライトを隠す
                // bit7 は CGB BG タイル優先度フラグ（Phase 4 で複合判定に拡張）
                if bg_priority && (self.bg_pixel_buffer[buf_idx] & 0x03) != 0 {
                    continue;
                }
                let color_idx = (palette >> (pixel << 1)) & 0b11;
                self.buffer[buf_idx] = DMG_PALETTE[color_idx as usize];
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
        let tile_map_base = if tile_map { 0x1C00 } else { 0x1800 };

        for i in win_x_start..LCD_WIDTH {
            let x = (i - win_x_start) as u8;
            let tile_row = (self.window_line_counter / 8) as usize;
            let tile_col = (x / 8) as usize;
            let tile_offset = (tile_row << 5) + tile_col;
            let tile_addr = tile_map_base + tile_offset;

            let raw_idx = self.vram[0][tile_addr];
            let tile_idx = if self.lcdc & TILE_DATA_ADDRESSING_MODE > 0 {
                raw_idx as usize
            } else {
                (0x100i16 + (raw_idx as i8) as i16) as usize
            };

            let pixel_row = self.window_line_counter & 7;
            let pixel_col = x & 7;
            let buf_idx = LCD_WIDTH * self.ly as usize + i;

            if self.cgb_mode {
                let attrs = self.vram[1][tile_addr];
                let palette_num = (attrs & 0x07) as usize;
                let vram_bank = ((attrs >> 3) & 0x01) as usize;
                let h_flip = attrs & 0x20 != 0;
                let v_flip = attrs & 0x40 != 0;
                let bg_prio = attrs & 0x80 != 0;

                let r = if v_flip { 7 - pixel_row } else { pixel_row };
                let c = if h_flip { 7 - pixel_col } else { pixel_col };
                let pixel = self.get_pixel_from_tile(tile_idx, r, c, vram_bank);

                self.bg_pixel_buffer[buf_idx] = pixel | if bg_prio { 0x80 } else { 0 };

                let palette_base = palette_num * 8 + pixel as usize * 2;
                let lo = self.bg_palette_ram[palette_base];
                let hi = self.bg_palette_ram[palette_base + 1];
                self.buffer[buf_idx] = lo as u16 | ((hi as u16) << 8);
            } else {
                let pixel = self.get_pixel_from_tile(tile_idx, pixel_row, pixel_col, 0);
                self.bg_pixel_buffer[buf_idx] = pixel;
                let color_idx = (self.bgp >> (pixel << 1)) & 0b11;
                self.buffer[buf_idx] = DMG_PALETTE[color_idx as usize];
            }
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

    pub fn pixel_buffer(&self) -> &[u16] {
        &self.buffer
    }
}
