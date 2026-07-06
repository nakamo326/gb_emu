//! PPU。スキャンライン単位で 240x160 の RGB555 フレームバッファへ描画する。
//!
//! 1 ライン = 1232 サイクル（可視 1006 + HBlank 226）、228 ライン（可視 160 + VBlank 68）。
//! HBlank 開始時に現在ラインを一括描画するスキャンライン方式。
//! 未実装: モザイク、HBlank Interval Free、サイクル単位のフェッチタイミング。

use alloc::boxed::Box;
use alloc::vec;

pub const WIDTH: usize = 240;
pub const HEIGHT: usize = 160;

const CYCLES_PER_LINE: u32 = 1232;
/// GBATEK: H-Blank フラグはサイクル 1006 で立つ
const HBLANK_START: u32 = 1006;
const TOTAL_LINES: u16 = 228;

/// ラインバッファの透過画素。色は 15bit なので bit15 を番兵に使う
const TRANSPARENT: u16 = 0x8000;

/// [`Ppu::step`] の結果。呼び出し側（Gba）が IF・DMA へ配線する。
#[derive(Default)]
pub struct PpuEvents {
    /// IF に立てるビット (bit0:vblank, bit1:hblank, bit2:vcount)
    pub irq: u16,
    /// 可視ラインの HBlank 開始（HBlank DMA のトリガー）
    pub hblank_dma: bool,
    /// VBlank 開始（VBlank DMA のトリガー）
    pub vblank_dma: bool,
    /// フレーム完成（VBlank 突入時）
    pub frame_done: bool,
}

pub struct Ppu {
    pub vram: Box<[u8]>,
    pub palette: Box<[u8]>,
    pub oam: Box<[u8]>,
    pub framebuffer: Box<[u16]>,

    dispcnt: u16,
    dispstat: u16,
    vcount: u16,
    bgcnt: [u16; 4],
    bghofs: [u16; 4],
    bgvofs: [u16; 4],
    // アフィン BG (BG2/BG3) パラメータ。添字 0=BG2, 1=BG3
    bgpa: [i16; 2],
    bgpb: [i16; 2],
    bgpc: [i16; 2],
    bgpd: [i16; 2],
    bgx: [i32; 2],
    bgy: [i32; 2],
    // ライン毎に PB/PD で進む内部リファレンス（フレーム先頭で BGX/BGY から再ロード）
    bgx_int: [i32; 2],
    bgy_int: [i32; 2],
    win0h: u16,
    win1h: u16,
    win0v: u16,
    win1v: u16,
    winin: u16,
    winout: u16,
    bldcnt: u16,
    bldalpha: u16,
    bldy: u16,

    line_cycles: u32,
    in_hblank: bool,
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            vram: vec![0; 0x1_8000].into_boxed_slice(),
            palette: vec![0; 0x400].into_boxed_slice(),
            oam: vec![0; 0x400].into_boxed_slice(),
            framebuffer: vec![0; WIDTH * HEIGHT].into_boxed_slice(),
            dispcnt: 0x80, // 起動直後は forced blank
            dispstat: 0,
            vcount: 0,
            bgcnt: [0; 4],
            bghofs: [0; 4],
            bgvofs: [0; 4],
            bgpa: [0x100; 2],
            bgpb: [0; 2],
            bgpc: [0; 2],
            bgpd: [0x100; 2],
            bgx: [0; 2],
            bgy: [0; 2],
            bgx_int: [0; 2],
            bgy_int: [0; 2],
            win0h: 0,
            win1h: 0,
            win0v: 0,
            win1v: 0,
            winin: 0,
            winout: 0,
            bldcnt: 0,
            bldalpha: 0,
            bldy: 0,
            line_cycles: 0,
            in_hblank: false,
        }
    }

    /// ビットマップモード（3-5）か。VRAM の OBJ 領域境界判定に使う
    pub fn bitmap_mode(&self) -> bool {
        self.dispcnt & 7 >= 3
    }

    pub fn step(&mut self, cycles: u32) -> PpuEvents {
        let mut ev = PpuEvents::default();
        self.line_cycles += cycles;

        if !self.in_hblank && self.line_cycles >= HBLANK_START {
            self.in_hblank = true;
            if self.vcount < HEIGHT as u16 {
                self.render_scanline();
                // アフィン内部リファレンスをライン毎に進める
                for i in 0..2 {
                    self.bgx_int[i] += self.bgpb[i] as i32;
                    self.bgy_int[i] += self.bgpd[i] as i32;
                }
                ev.hblank_dma = true;
            }
            if self.dispstat & 0x10 != 0 {
                ev.irq |= 0x02;
            }
        }

        if self.line_cycles >= CYCLES_PER_LINE {
            self.line_cycles -= CYCLES_PER_LINE;
            self.in_hblank = false;
            self.vcount += 1;
            if self.vcount == TOTAL_LINES {
                self.vcount = 0;
                // フレーム先頭でアフィン内部リファレンスを再ロード
                self.bgx_int = self.bgx;
                self.bgy_int = self.bgy;
            }
            if self.vcount == HEIGHT as u16 {
                ev.frame_done = true;
                ev.vblank_dma = true;
                if self.dispstat & 0x08 != 0 {
                    ev.irq |= 0x01;
                }
            }
            if self.vcount == self.dispstat >> 8 && self.dispstat & 0x20 != 0 {
                ev.irq |= 0x04;
            }
        }
        ev
    }

    pub fn read_io(&self, off: usize) -> u16 {
        match off {
            0x00 => self.dispcnt,
            0x04 => {
                let mut v = self.dispstat & 0xFFB8;
                // VBlank フラグはライン 160-226 で立つ（227 では立たない）
                if (HEIGHT as u16..TOTAL_LINES - 1).contains(&self.vcount) {
                    v |= 0x01;
                }
                if self.line_cycles >= HBLANK_START {
                    v |= 0x02;
                }
                if self.vcount == self.dispstat >> 8 {
                    v |= 0x04;
                }
                v
            }
            0x06 => self.vcount,
            0x08 | 0x0A | 0x0C | 0x0E => self.bgcnt[(off - 0x08) / 2],
            0x48 => self.winin,
            0x4A => self.winout,
            0x50 => self.bldcnt,
            0x52 => self.bldalpha,
            _ => 0,
        }
    }

    pub fn write_io(&mut self, off: usize, val: u16, mask: u16) {
        let merge = |cur: u16| cur & !mask | val & mask;
        match off {
            0x00 => self.dispcnt = merge(self.dispcnt),
            0x04 => self.dispstat = merge(self.dispstat) & 0xFFB8,
            0x08 | 0x0A | 0x0C | 0x0E => {
                let i = (off - 0x08) / 2;
                self.bgcnt[i] = merge(self.bgcnt[i]);
            }
            0x10 | 0x14 | 0x18 | 0x1C => {
                let i = (off - 0x10) / 4;
                self.bghofs[i] = merge(self.bghofs[i]) & 0x1FF;
            }
            0x12 | 0x16 | 0x1A | 0x1E => {
                let i = (off - 0x12) / 4;
                self.bgvofs[i] = merge(self.bgvofs[i]) & 0x1FF;
            }
            0x20 | 0x30 => self.bgpa[(off - 0x20) / 0x10] = merge_i16(self.bgpa[(off - 0x20) / 0x10], val, mask),
            0x22 | 0x32 => self.bgpb[(off - 0x22) / 0x10] = merge_i16(self.bgpb[(off - 0x22) / 0x10], val, mask),
            0x24 | 0x34 => self.bgpc[(off - 0x24) / 0x10] = merge_i16(self.bgpc[(off - 0x24) / 0x10], val, mask),
            0x26 | 0x36 => self.bgpd[(off - 0x26) / 0x10] = merge_i16(self.bgpd[(off - 0x26) / 0x10], val, mask),
            0x28 | 0x2A | 0x38 | 0x3A => {
                let i = if off >= 0x38 { 1 } else { 0 };
                let hi = off & 2 != 0;
                self.bgx[i] = merge_ref(self.bgx[i], val, mask, hi);
                self.bgx_int[i] = self.bgx[i];
            }
            0x2C | 0x2E | 0x3C | 0x3E => {
                let i = if off >= 0x3C { 1 } else { 0 };
                let hi = off & 2 != 0;
                self.bgy[i] = merge_ref(self.bgy[i], val, mask, hi);
                self.bgy_int[i] = self.bgy[i];
            }
            0x40 => self.win0h = merge(self.win0h),
            0x42 => self.win1h = merge(self.win1h),
            0x44 => self.win0v = merge(self.win0v),
            0x46 => self.win1v = merge(self.win1v),
            0x48 => self.winin = merge(self.winin) & 0x3F3F,
            0x4A => self.winout = merge(self.winout) & 0x3F3F,
            // 0x4C: MOSAIC は未実装
            0x50 => self.bldcnt = merge(self.bldcnt) & 0x3FFF,
            0x52 => self.bldalpha = merge(self.bldalpha) & 0x1F1F,
            0x54 => self.bldy = merge(self.bldy) & 0x1F,
            _ => {}
        }
    }

    fn pal_color(&self, idx: usize) -> u16 {
        (self.palette[idx * 2] as u16 | (self.palette[idx * 2 + 1] as u16) << 8) & 0x7FFF
    }

    fn render_scanline(&mut self) {
        let ly = self.vcount as usize;
        let mut out = [0u16; WIDTH];

        if self.dispcnt & 0x80 != 0 {
            // forced blank は白
            out.fill(0x7FFF);
            self.framebuffer[ly * WIDTH..][..WIDTH].copy_from_slice(&out);
            return;
        }

        let mode = self.dispcnt & 7;
        let mut bg = [[TRANSPARENT; WIDTH]; 4];
        let bg_on = |n: usize| self.dispcnt & (0x100 << n) != 0;
        match mode {
            0 => {
                for n in 0..4 {
                    if bg_on(n) {
                        self.render_text_bg(n, ly, &mut bg[n]);
                    }
                }
            }
            1 => {
                for n in 0..2 {
                    if bg_on(n) {
                        self.render_text_bg(n, ly, &mut bg[n]);
                    }
                }
                if bg_on(2) {
                    self.render_affine_bg(2, &mut bg[2]);
                }
            }
            2 => {
                for n in 2..4 {
                    if bg_on(n) {
                        self.render_affine_bg(n, &mut bg[n]);
                    }
                }
            }
            3..=5 => {
                if bg_on(2) {
                    self.render_bitmap(mode, ly, &mut bg[2]);
                }
            }
            _ => {}
        }

        let mut obj_color = [TRANSPARENT; WIDTH];
        let mut obj_prio = [4u8; WIDTH];
        let mut obj_semi = [false; WIDTH];
        let mut obj_win = [false; WIDTH];
        if self.dispcnt & 0x1000 != 0 {
            self.render_objs(ly, &mut obj_color, &mut obj_prio, &mut obj_semi, &mut obj_win);
        }

        let backdrop = self.pal_color(0);
        // このモードで有効になり得る BG（優先度タイの解決を BG 番号順で行うため昇順）
        let bg_avail: &[usize] = match mode {
            0 => &[0, 1, 2, 3],
            1 => &[0, 1, 2],
            2 => &[2, 3],
            _ => &[2],
        };
        let windows_active = self.dispcnt & 0xE000 != 0;

        for x in 0..WIDTH {
            let (enable, blend_ok) = if windows_active {
                self.window_ctl(x, ly, obj_win[x])
            } else {
                (0x3F, true)
            };

            // 優先度順に最前面と 2 番目のレイヤーを求める（レイヤー番号: 0-3=BG, 4=OBJ, 5=バックドロップ）
            let mut layers = [(backdrop, 5usize); 2];
            let mut found = 0;
            'search: for prio in 0..4u8 {
                if enable & 0x10 != 0 && obj_prio[x] == prio && obj_color[x] != TRANSPARENT {
                    layers[found] = (obj_color[x], 4);
                    found += 1;
                    if found == 2 {
                        break 'search;
                    }
                }
                for &n in bg_avail {
                    if enable & (1 << n) != 0
                        && self.bgcnt[n] & 3 == prio as u16
                        && bg[n][x] != TRANSPARENT
                    {
                        layers[found] = (bg[n][x], n);
                        found += 1;
                        if found == 2 {
                            break 'search;
                        }
                    }
                }
            }

            let (top_color, top_layer) = layers[0];
            let (bot_color, bot_layer) = layers[1];
            let top_sel = self.bldcnt & (1 << top_layer) != 0;
            let bot_sel = self.bldcnt & (0x100 << bot_layer) != 0;
            let eva = (self.bldalpha & 0x1F).min(16) as u32;
            let evb = (self.bldalpha >> 8 & 0x1F).min(16) as u32;

            let mut color = top_color;
            if blend_ok {
                if top_layer == 4 && obj_semi[x] && bot_sel {
                    // 半透明 OBJ は BLDCNT のモードに関係なく αブレンド
                    color = alpha_blend(top_color, bot_color, eva, evb);
                } else if top_sel {
                    match self.bldcnt >> 6 & 3 {
                        1 if bot_sel => color = alpha_blend(top_color, bot_color, eva, evb),
                        2 => color = fade(top_color, self.bldy as u32, true),
                        3 => color = fade(top_color, self.bldy as u32, false),
                        _ => {}
                    }
                }
            }
            out[x] = color;
        }

        self.framebuffer[ly * WIDTH..][..WIDTH].copy_from_slice(&out);
    }

    /// ウィンドウ判定。返り値: (レイヤー有効ビット bit0-4, 色特殊効果の可否)
    fn window_ctl(&self, x: usize, y: usize, obj_win: bool) -> (u16, bool) {
        let ctl = if self.dispcnt & 0x2000 != 0 && in_window(self.win0h, self.win0v, x, y) {
            self.winin & 0x3F
        } else if self.dispcnt & 0x4000 != 0 && in_window(self.win1h, self.win1v, x, y) {
            self.winin >> 8
        } else if self.dispcnt & 0x8000 != 0 && obj_win {
            self.winout >> 8
        } else {
            self.winout & 0x3F
        };
        (ctl & 0x1F, ctl & 0x20 != 0)
    }

    fn render_text_bg(&self, n: usize, ly: usize, buf: &mut [u16; WIDTH]) {
        let cnt = self.bgcnt[n];
        let char_base = ((cnt >> 2 & 3) as usize) * 0x4000;
        let screen_base = ((cnt >> 8 & 0x1F) as usize) * 0x800;
        let bpp8 = cnt & 0x80 != 0;
        let w_mask = if cnt & 0x4000 != 0 { 511 } else { 255 };
        let h_mask = if cnt & 0x8000 != 0 { 511 } else { 255 };
        let py = (ly + self.bgvofs[n] as usize) & h_mask;

        for x in 0..WIDTH {
            let px = (x + self.bghofs[n] as usize) & w_mask;
            // 256x256 のスクリーンブロック単位で分割配置される
            let block = (px >> 8) + (py >> 8) * if w_mask == 511 { 2 } else { 1 };
            let entry_off = screen_base + block * 0x800 + (((py & 255) >> 3) * 32 + ((px & 255) >> 3)) * 2;
            let entry = self.vram[entry_off] as u16 | (self.vram[entry_off + 1] as u16) << 8;
            let tile = (entry & 0x3FF) as usize;
            let mut tx = px & 7;
            let mut ty = py & 7;
            if entry & 0x400 != 0 {
                tx = 7 - tx;
            }
            if entry & 0x800 != 0 {
                ty = 7 - ty;
            }
            let idx = if bpp8 {
                let off = char_base + tile * 64 + ty * 8 + tx;
                if off >= 0x1_0000 {
                    continue; // BG キャラクタ領域 (64KB) 外
                }
                self.vram[off] as usize
            } else {
                let off = char_base + tile * 32 + ty * 4 + tx / 2;
                if off >= 0x1_0000 {
                    continue;
                }
                let b = self.vram[off] as usize;
                let nib = if tx & 1 != 0 { b >> 4 } else { b & 0xF };
                if nib == 0 { 0 } else { (entry >> 12) as usize * 16 + nib }
            };
            if idx != 0 {
                buf[x] = self.pal_color(idx);
            }
        }
    }

    fn render_affine_bg(&self, n: usize, buf: &mut [u16; WIDTH]) {
        let i = n - 2;
        let cnt = self.bgcnt[n];
        let char_base = ((cnt >> 2 & 3) as usize) * 0x4000;
        let screen_base = ((cnt >> 8 & 0x1F) as usize) * 0x800;
        let size = 128i32 << (cnt >> 14);
        let wrap = cnt & 0x2000 != 0;
        let mut fx = self.bgx_int[i];
        let mut fy = self.bgy_int[i];

        for x in 0..WIDTH {
            let mut px = fx >> 8;
            let mut py = fy >> 8;
            fx += self.bgpa[i] as i32;
            fy += self.bgpc[i] as i32;
            if wrap {
                px &= size - 1;
                py &= size - 1;
            } else if px < 0 || px >= size || py < 0 || py >= size {
                continue;
            }
            let tile = self.vram[screen_base + (py as usize / 8) * (size as usize / 8) + px as usize / 8] as usize;
            let off = char_base + tile * 64 + (py as usize & 7) * 8 + (px as usize & 7);
            if off >= 0x1_0000 {
                continue;
            }
            let idx = self.vram[off] as usize;
            if idx != 0 {
                buf[x] = self.pal_color(idx);
            }
        }
    }

    fn render_bitmap(&self, mode: u16, ly: usize, buf: &mut [u16; WIDTH]) {
        // モード 4/5 は DISPCNT bit4 でページフリップ
        let page = if self.dispcnt & 0x10 != 0 { 0xA000 } else { 0 };
        match mode {
            3 => {
                for x in 0..WIDTH {
                    let off = (ly * WIDTH + x) * 2;
                    buf[x] = (self.vram[off] as u16 | (self.vram[off + 1] as u16) << 8) & 0x7FFF;
                }
            }
            4 => {
                for x in 0..WIDTH {
                    let idx = self.vram[page + ly * WIDTH + x] as usize;
                    if idx != 0 {
                        buf[x] = self.pal_color(idx);
                    }
                }
            }
            5 => {
                // 160x128 の縮小フレーム
                if ly < 128 {
                    for x in 0..160 {
                        let off = page + (ly * 160 + x) * 2;
                        buf[x] = (self.vram[off] as u16 | (self.vram[off + 1] as u16) << 8) & 0x7FFF;
                    }
                }
            }
            _ => {}
        }
    }

    fn render_objs(
        &self,
        ly: usize,
        color: &mut [u16; WIDTH],
        prio: &mut [u8; WIDTH],
        semi: &mut [bool; WIDTH],
        obj_win: &mut [bool; WIDTH],
    ) {
        // OAM インデックス昇順に走査し、同一ピクセルは優先度値が小さい方
        // （同値なら先勝ち = 若いインデックス）を採用する
        for i in 0..128 {
            let attr0 = self.oam[i * 8] as u16 | (self.oam[i * 8 + 1] as u16) << 8;
            let attr1 = self.oam[i * 8 + 2] as u16 | (self.oam[i * 8 + 3] as u16) << 8;
            let attr2 = self.oam[i * 8 + 4] as u16 | (self.oam[i * 8 + 5] as u16) << 8;

            let affine = attr0 & 0x100 != 0;
            if !affine && attr0 & 0x200 != 0 {
                continue; // 非表示
            }
            let mode = attr0 >> 10 & 3;
            if mode == 3 {
                continue; // 禁止値
            }
            let shape = (attr0 >> 14) as usize;
            let size = (attr1 >> 14) as usize;
            let (w, h) = OBJ_SIZES[shape][size];
            let (bw, bh) = if affine && attr0 & 0x200 != 0 { (w * 2, h * 2) } else { (w, h) };

            let mut top = (attr0 & 0xFF) as i32;
            if top + bh > 256 {
                top -= 256;
            }
            let row = ly as i32 - top;
            if row < 0 || row >= bh {
                continue;
            }
            let mut left = (attr1 & 0x1FF) as i32;
            if left >= 256 {
                left -= 512;
            }

            let bpp8 = attr0 & 0x2000 != 0;
            let base_tile = (attr2 & 0x3FF) as usize;
            let obj_prio = (attr2 >> 10 & 3) as u8;
            let pal_bank = (attr2 >> 12) as usize;
            let map_1d = self.dispcnt & 0x40 != 0;
            // 1 行あたりのタイル番号の増分（タイル番号は常に 32 バイト単位）
            let row_stride = if map_1d { w as usize / 8 * if bpp8 { 2 } else { 1 } } else { 32 };

            // アフィンパラメータ（OAM 内に 8 バイト間隔で格納されている）
            let params = if affine {
                let g = (attr1 >> 9 & 0x1F) as usize * 32;
                let p = |k: usize| {
                    (self.oam[g + k * 8 + 6] as u16 | (self.oam[g + k * 8 + 7] as u16) << 8) as i16 as i32
                };
                Some((p(0), p(1), p(2), p(3)))
            } else {
                None
            };

            for col in 0..bw {
                let sx = left + col;
                if !(0..WIDTH as i32).contains(&sx) {
                    continue;
                }
                let sx = sx as usize;
                if mode != 2 && color[sx] != TRANSPARENT && prio[sx] <= obj_prio {
                    continue;
                }

                let (tx, ty) = match params {
                    Some((pa, pb, pc, pd)) => {
                        let lx = col - bw / 2;
                        let lyy = row - bh / 2;
                        let tx = ((pa * lx + pb * lyy) >> 8) + w / 2;
                        let ty = ((pc * lx + pd * lyy) >> 8) + h / 2;
                        if tx < 0 || tx >= w || ty < 0 || ty >= h {
                            continue;
                        }
                        (tx as usize, ty as usize)
                    }
                    None => {
                        let tx = if attr1 & 0x1000 != 0 { w - 1 - col } else { col };
                        let ty = if attr1 & 0x2000 != 0 { h - 1 - row } else { row };
                        (tx as usize, ty as usize)
                    }
                };

                let tile_step = if bpp8 { 2 } else { 1 };
                let tile = base_tile + ty / 8 * row_stride + tx / 8 * tile_step;
                // ビットマップモードではタイル 0-511 の領域が BG に使われるため参照不可
                if self.bitmap_mode() && tile < 512 {
                    continue;
                }
                let off = 0x1_0000 + (tile & 0x3FF) * 32
                    + if bpp8 { (ty & 7) * 8 + (tx & 7) } else { (ty & 7) * 4 + (tx & 7) / 2 };
                let idx = if bpp8 {
                    self.vram[off] as usize
                } else {
                    let b = self.vram[off] as usize;
                    let nib = if tx & 1 != 0 { b >> 4 } else { b & 0xF };
                    if nib == 0 { 0 } else { pal_bank * 16 + nib }
                };
                if idx == 0 {
                    continue;
                }
                if mode == 2 {
                    obj_win[sx] = true;
                } else {
                    color[sx] = self.pal_color(256 + idx);
                    prio[sx] = obj_prio;
                    semi[sx] = mode == 1;
                }
            }
        }
    }
}

const OBJ_SIZES: [[(i32, i32); 4]; 3] = [
    [(8, 8), (16, 16), (32, 32), (64, 64)],   // 正方形
    [(16, 8), (32, 8), (32, 16), (64, 32)],   // 横長
    [(8, 16), (8, 32), (16, 32), (32, 64)],   // 縦長
];

/// WINxH/WINxV の範囲判定。x1 > x2 のときは画面端をまたぐ
fn in_window(h: u16, v: u16, x: usize, y: usize) -> bool {
    let (x1, x2) = ((h >> 8) as usize, (h & 0xFF) as usize);
    let (y1, y2) = ((v >> 8) as usize, (v & 0xFF) as usize);
    let in_h = if x1 <= x2 { x1 <= x && x < x2 } else { x >= x1 || x < x2 };
    let in_v = if y1 <= y2 { y1 <= y && y < y2 } else { y >= y1 || y < y2 };
    in_h && in_v
}

fn alpha_blend(top: u16, bot: u16, eva: u32, evb: u32) -> u16 {
    let mut out = 0u16;
    for shift in [0, 5, 10] {
        let a = (top >> shift & 0x1F) as u32;
        let b = (bot >> shift & 0x1F) as u32;
        out |= (((a * eva + b * evb) / 16).min(31) as u16) << shift;
    }
    out
}

/// evy による白フェード（to_white=true）/ 黒フェード
fn fade(color: u16, evy: u32, to_white: bool) -> u16 {
    let evy = evy.min(16);
    let mut out = 0u16;
    for shift in [0, 5, 10] {
        let c = (color >> shift & 0x1F) as u32;
        let v = if to_white { c + (31 - c) * evy / 16 } else { c - c * evy / 16 };
        out |= (v.min(31) as u16) << shift;
    }
    out
}

/// BG リファレンスレジスタ（28bit 符号付き固定小数点）の半語書き込みマージ
fn merge_ref(cur: i32, val: u16, mask: u16, hi: bool) -> i32 {
    let (val, mask) = if hi {
        ((val as u32) << 16, (mask as u32) << 16)
    } else {
        (val as u32, mask as u32)
    };
    let raw = (cur as u32) & !mask | val & mask;
    // 28bit を 32bit へ符号拡張
    ((raw << 4) as i32) >> 4
}

fn merge_i16(cur: i16, val: u16, mask: u16) -> i16 {
    (cur as u16 & !mask | val & mask) as i16
}
