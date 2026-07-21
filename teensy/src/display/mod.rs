pub mod font;
pub mod panel;

use core::marker::PhantomData;

use gb_core::platform::Display;

use cortex_m::peripheral::DWT;
use embedded_hal::blocking::spi::Write as SpiWrite;
use embedded_hal::digital::v2::OutputPin;

use bsp::hal;
use hal::dma::channel::{self, Channel, Configuration};
use panel::PanelController;
use teensy4_bsp as bsp;

const GB_W: usize = 160;
const GB_H: usize = 144;
const PIXEL_COUNT: usize = GB_W * GB_H; // 23040
const BUF_U32: usize = PIXEL_COUNT / 2; // 11520 — 2px を 1 u32 にパック

const CASET: u8 = 0x2A;
const RASET: u8 = 0x2B;
const RAMWR: u8 = 0x2C;

const SPI_MAX_CHUNK: usize = 512;

// --- 情報オーバーレイ (FPS など) ---
const OVERLAY_SCALE: u16 = 2; // フォント拡大率 (5x7 → 10x14)
const OVERLAY_X: u16 = 4; // GB 領域上の余白バンド内の描画位置
const OVERLAY_Y: u16 = 12;
const OVERLAY_FG: u16 = rgb565(0xFF, 0xFF, 0xFF);
const OVERLAY_BG: u16 = rgb565(0x00, 0x00, 0x00);
// 1 グリフ行ぶんの一時バッファ最大バイト数。
// セル幅 (font::WIDTH + 1) * scale * 2byte。scale<=4 で 48byte に収まる。
const GLYPH_ROW_BYTES: usize = (font::WIDTH + 1) * 4 * 2;

const fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | ((b as u16) >> 3)
}

/// RGB555 (bits 0-4=R, 5-9=G, 10-14=B) → RGB565 変換
#[inline(always)]
const fn rgb555_to_rgb565(px: u16) -> u16 {
    let r5 = px & 0x1F;
    let g5 = (px >> 5) & 0x1F;
    let b5 = (px >> 10) & 0x1F;
    // G を 5bit→6bit に拡張（MSB を LSB に複製）
    let g6 = (g5 << 1) | (g5 >> 4);
    (r5 << 11) | (g6 << 5) | b5
}

// LPSPI4 TX の DMA ソース番号 (i.MX RT1060)
const LPSPI4_TX_DMA_SOURCE: u32 = 80;

static mut FB: [[u32; BUF_U32]; 2] = [[0; BUF_U32]; 2];

pub struct DmaDisplay<P, SPI, DC, RST> {
    spi: SPI,
    dc: DC,
    rst: RST,
    channel: Channel,
    back: usize,
    in_flight: bool,
    // --- FPS 計測 ---
    frame_count: u32,      // 計測ウィンドウ内の描画フレーム数
    last_fps_cycle: u32,   // 直近のウィンドウ開始時の DWT サイクル
    fps_tenths: u32,       // 最新の確定 FPS 値 (×10, 0.1fps 単位)
    fps_dirty: bool,       // 表示更新が必要か (値が変わった時のみ true)
    // --- フレーム負荷計測 (スパイク/コマ落ち診断用) ---
    work_max: u32,         // 現ウィンドウ内の最悪フレーム処理サイクル
    work_sum: u64,         // 同・合計 (平均算出用)
    work_samples: u32,     // 同・サンプル数 (フレーム数)
    drop_count: u32,       // 同・予算超過したフレーム数
    budget: u32,           // 直近の 1 フレーム予算サイクル (record_work から受領)
    peak_pct: u32,         // 確定値: 最悪負荷 (予算比 %)
    avg_pct: u32,          // 確定値: 平均負荷 (予算比 %)
    drops: u32,            // 確定値: 直近 1 秒のコマ落ち回数
    _panel: PhantomData<P>,
}

impl<P, SPI, DC, RST> DmaDisplay<P, SPI, DC, RST>
where
    P: PanelController,
    SPI: SpiWrite<u8>,
    DC: OutputPin,
    RST: OutputPin,
{
    pub fn new(spi: SPI, dc: DC, rst: RST, channel: Channel) -> Self {
        let mut d = Self {
            spi,
            dc,
            rst,
            channel,
            back: 0,
            in_flight: false,
            frame_count: 0,
            last_fps_cycle: DWT::cycle_count(),
            fps_tenths: 0,
            fps_dirty: false,
            work_max: 0,
            work_sum: 0,
            work_samples: 0,
            drop_count: 0,
            budget: 0,
            peak_pct: 0,
            avg_pct: 0,
            drops: 0,
            _panel: PhantomData,
        };
        d.channel.reset();
        d.channel.set_disable_on_completion(true);
        d.init_panel();
        d
    }

    fn delay_ms(ms: u32) {
        cortex_m::asm::delay(bsp::board::ARM_FREQUENCY / 1_000 / 3 * ms);
    }

    fn cmd(&mut self, c: u8) {
        let _ = self.dc.set_low();
        let _ = self.spi.write(&[c]);
    }

    fn data_pio(&mut self, d: &[u8]) {
        let _ = self.dc.set_high();
        let _ = self.spi.write(d);
    }

    fn init_panel(&mut self) {
        let _ = self.rst.set_low();
        Self::delay_ms(10);
        let _ = self.rst.set_high();
        Self::delay_ms(120);

        for step in P::init_sequence() {
            self.cmd(step.cmd);
            if !step.data.is_empty() {
                self.data_pio(step.data);
            }
            if step.delay_ms > 0 {
                Self::delay_ms(step.delay_ms);
            }
        }

        self.clear_screen();
    }

    fn clear_screen(&mut self) {
        self.cmd(CASET);
        self.data_pio(&[0, 0, ((P::WIDTH - 1) >> 8) as u8, (P::WIDTH - 1) as u8]);
        self.cmd(RASET);
        self.data_pio(&[0, 0, ((P::HEIGHT - 1) >> 8) as u8, (P::HEIGHT - 1) as u8]);
        self.cmd(RAMWR);
        let _ = self.dc.set_high();

        let black = [0u8; SPI_MAX_CHUNK];
        let total = P::WIDTH as usize * P::HEIGHT as usize * 2;
        let mut sent = 0;
        while sent < total {
            let n = (total - sent).min(SPI_MAX_CHUNK);
            let _ = self.spi.write(&black[..n]);
            sent += n;
        }
    }

    /// 描画ウィンドウ (CASET/RASET) を設定し RAMWR を発行する。
    /// `x`/`y` はパネルオフセットを含まない論理座標。
    fn set_rect(&mut self, x: u16, y: u16, w: u16, h: u16) {
        let x0 = x + P::COL_OFFSET;
        let y0 = y + P::ROW_OFFSET;
        let x1 = x0 + w - 1;
        let y1 = y0 + h - 1;

        self.cmd(CASET);
        self.data_pio(&[(x0 >> 8) as u8, x0 as u8, (x1 >> 8) as u8, x1 as u8]);
        self.cmd(RASET);
        self.data_pio(&[(y0 >> 8) as u8, y0 as u8, (y1 >> 8) as u8, y1 as u8]);
        self.cmd(RAMWR);
    }

    fn set_window(&mut self) {
        let x0 = (P::WIDTH - GB_W as u16) / 2;
        let y0 = (P::HEIGHT - GB_H as u16) / 2;
        self.set_rect(x0, y0, GB_W as u16, GB_H as u16);
    }

    /// 1 文字を PIO (ブロッキング) で描画する。GB の DMA が走っていない
    /// 区間でのみ呼ぶこと (SPI バス競合を避けるため)。
    fn draw_char(&mut self, x: u16, y: u16, ch: u8, scale: u16) {
        let g = font::glyph(ch);
        let cols = font::WIDTH as u16 + 1; // グリフ 5 列 + 右スペース 1 列
        let rows = font::HEIGHT as u16 + 1; // グリフ 7 行 + 下スペース 1 行
        self.set_rect(x, y, cols * scale, rows * scale);
        let _ = self.dc.set_high();

        let fg = OVERLAY_FG.to_be_bytes();
        let bg = OVERLAY_BG.to_be_bytes();
        let mut row_buf = [0u8; GLYPH_ROW_BYTES];

        for ry in 0..rows {
            let bits = if ry < font::HEIGHT as u16 {
                g[ry as usize]
            } else {
                0
            };
            // この行 (拡大前 1 ライン) を RGB565 で展開
            let mut n = 0;
            for cx in 0..cols {
                let on = cx < font::WIDTH as u16
                    && (bits >> (font::WIDTH as u16 - 1 - cx)) & 1 != 0;
                let px = if on { fg } else { bg };
                for _ in 0..scale {
                    row_buf[n] = px[0];
                    row_buf[n + 1] = px[1];
                    n += 2;
                }
            }
            // 縦方向の拡大ぶん同じ行を繰り返し送出
            for _ in 0..scale {
                let _ = self.spi.write(&row_buf[..n]);
            }
        }
    }

    fn draw_text(&mut self, x: u16, y: u16, text: &[u8], scale: u16) {
        let cell_w = (font::WIDTH as u16 + 1) * scale;
        let mut cx = x;
        for &ch in text {
            self.draw_char(cx, y, ch, scale);
            cx += cell_w;
        }
    }

    /// 1 フレーム分の実処理サイクル (step 群 + draw、ビジーウェイトを除く) を記録する。
    /// `budget` は 1 フレームの予算サイクル。main ループから毎フレーム呼ぶ。
    pub fn record_work(&mut self, cycles: u32, budget: u32) {
        self.budget = budget;
        if cycles > self.work_max {
            self.work_max = cycles;
        }
        self.work_sum += cycles as u64;
        self.work_samples += 1;
        if cycles > budget {
            self.drop_count += 1;
        }
    }

    /// フレーム毎に呼び出して FPS とフレーム負荷統計を更新する。
    /// 1 秒経過ごとに値を確定し、変化があれば `fps_dirty` を立てる。
    fn update_fps(&mut self) {
        self.frame_count += 1;
        let now = DWT::cycle_count();
        let elapsed = now.wrapping_sub(self.last_fps_cycle);
        if elapsed >= bsp::board::ARM_FREQUENCY {
            // fps = frames / (elapsed / ARM_FREQ)。0.1fps 単位に丸めるため ×10。
            // 600MHz × 60frame × 10 は u32 を溢れるので u64 で計算する。
            let tenths = (self.frame_count as u64 * bsp::board::ARM_FREQUENCY as u64 * 10
                / elapsed as u64) as u32;
            if tenths != self.fps_tenths {
                self.fps_tenths = tenths;
                self.fps_dirty = true;
            }
            self.frame_count = 0;
            self.last_fps_cycle = now;

            // フレーム負荷統計を確定 (予算比 %)。
            let budget = self.budget.max(1) as u64;
            let peak = (self.work_max as u64 * 100 / budget) as u32;
            let avg = if self.work_samples > 0 {
                (self.work_sum * 100 / (self.work_samples as u64 * budget)) as u32
            } else {
                0
            };
            if peak != self.peak_pct || avg != self.avg_pct || self.drop_count != self.drops {
                self.peak_pct = peak;
                self.avg_pct = avg;
                self.drops = self.drop_count;
                self.fps_dirty = true;
            }
            self.work_max = 0;
            self.work_sum = 0;
            self.work_samples = 0;
            self.drop_count = 0;
        }
    }

    /// 情報オーバーレイ (現状 FPS のみ) を余白バンドに描画する。
    fn render_overlay(&mut self) {
        let mut text = [b' '; 10];
        text[0] = b'F';
        text[1] = b'P';
        text[2] = b'S';
        text[3] = b':';
        let mut len = 4;

        // 整数部 (0.1fps 単位の値を 10 で割る)。最大 999.9fps 想定。
        let int = (self.fps_tenths / 10).min(999);
        if int >= 100 {
            text[len] = b'0' + (int / 100 % 10) as u8;
            len += 1;
        }
        if int >= 10 {
            text[len] = b'0' + (int / 10 % 10) as u8;
            len += 1;
        }
        text[len] = b'0' + (int % 10) as u8;
        len += 1;
        // 小数第1位
        text[len] = b'.';
        len += 1;
        text[len] = b'0' + (self.fps_tenths % 10) as u8;
        len += 1;

        self.draw_text(OVERLAY_X, OVERLAY_Y, &text[..len], OVERLAY_SCALE);

        // 2 行目: フレーム負荷統計  "P<peak>% A<avg>% D<drops>"
        //   P = 最悪負荷 (予算比 %)、A = 平均負荷、D = 直近 1 秒のコマ落ち回数
        let mut s = [b' '; 20];
        let mut n = 0;
        s[n] = b'P';
        n += 1;
        n = Self::put_num(&mut s, n, self.peak_pct.min(999));
        s[n] = b'%';
        n += 1;
        s[n] = b' ';
        n += 1;
        s[n] = b'A';
        n += 1;
        n = Self::put_num(&mut s, n, self.avg_pct.min(999));
        s[n] = b'%';
        n += 1;
        s[n] = b' ';
        n += 1;
        s[n] = b'D';
        n += 1;
        n = Self::put_num(&mut s, n, self.drops.min(999));

        let line2_y = OVERLAY_Y + (font::HEIGHT as u16 + 1) * OVERLAY_SCALE;
        self.draw_text(OVERLAY_X, line2_y, &s[..n], OVERLAY_SCALE);
    }

    /// 10 進数 (0..=999) を buf[pos..] に書き、次の書き込み位置を返す。
    fn put_num(buf: &mut [u8], mut pos: usize, v: u32) -> usize {
        if v >= 100 {
            buf[pos] = b'0' + ((v / 100) % 10) as u8;
            pos += 1;
        }
        if v >= 10 {
            buf[pos] = b'0' + ((v / 10) % 10) as u8;
            pos += 1;
        }
        buf[pos] = b'0' + (v % 10) as u8;
        pos += 1;
        pos
    }

    fn wait_dma_complete(&mut self) {
        if !self.in_flight {
            return;
        }
        while !self.channel.is_complete() {}
        self.channel.clear_complete();
        self.channel.disable();
        self.finalize_continuous_transfer();
        self.in_flight = false;
    }

    fn fill_buffer(buf: &mut [u32; BUF_U32], pixels: &[u16]) {
        let len = pixels.len().min(PIXEL_COUNT);
        let pairs = len / 2;
        for i in 0..pairs {
            let c0 = rgb555_to_rgb565(pixels[i * 2]);
            let c1 = rgb555_to_rgb565(pixels[i * 2 + 1]);
            buf[i] = (c0 as u32) << 16 | c1 as u32;
        }
        if len & 1 != 0 {
            let c = rgb555_to_rgb565(pixels[len - 1]);
            buf[pairs] = (c as u32) << 16;
        }
    }

    fn start_dma(&mut self) {
        let front = 1 - self.back;
        let buf = unsafe { &FB[front] };

        self.channel
            .set_channel_configuration(Configuration::enable(LPSPI4_TX_DMA_SOURCE));

        unsafe {
            channel::set_source_linear_buffer(&mut self.channel, buf);
            // TDR アドレスを destination に設定。
            // LPSPI Destination<u32> trait の destination_address() と同等だが、
            // Lpspi の &mut を DMA チャネルに渡せないため手動で設定する。
            // spi フィールドは embedded-hal blocking trait 経由でしか使わないため、
            // ここでは board::Lpspi4 として直接 tdr() を呼べない。
            // 代わりに LPSPI4 TDR の既知アドレスを使用する。
            // i.MX RT1060: LPSPI4 base = 0x403A_0000, TDR offset = 0x64
            const LPSPI4_TDR: *const u32 = 0x403A_0064 as *const u32;
            channel::set_destination_hardware::<u32>(&mut self.channel, LPSPI4_TDR);

            self.channel.set_minor_loop_bytes(4);
            self.channel.set_transfer_iterations(BUF_U32 as u16);
        }

        // LPSPI4 の DMA TX 要求を有効化 (DER.TDDE = 1, FCR.TXWATER = 0)
        // Lpspi::enable_dma_transmit() と同等の処理をレジスタ直接操作で行う
        unsafe {
            const LPSPI4_BASE: u32 = 0x403A_0000;
            let fcr = (LPSPI4_BASE + 0x58) as *mut u32;
            let der = (LPSPI4_BASE + 0x1C) as *mut u32;

            // TXWATER = 0 (FIFO が空になるたびに DMA リクエスト)
            let fcr_val = core::ptr::read_volatile(fcr);
            core::ptr::write_volatile(fcr, fcr_val & !0x0F);

            // TDDE = 1 (Transmit Data DMA Enable)
            let der_val = core::ptr::read_volatile(der);
            core::ptr::write_volatile(der, der_val | 1);
        }

        // TCR: 32-bit frame, continuous mode, RX mask, MSB first
        // enqueue_transaction が使えないため TCR を直接書く
        unsafe {
            const LPSPI4_TCR: *mut u32 = (0x403A_0000 + 0x60) as *mut u32;
            // FRAMESZ=31, RXMSK=1(bit 19), CONT=1(bit 21), PCS=0, WIDTH=0, LSBF=0
            let tcr_val: u32 = 31 // FRAMESZ = 31 (32 bits)
                | (1 << 19)       // RXMSK
                | (1 << 21); // CONT
            core::ptr::write_volatile(LPSPI4_TCR, tcr_val);
        }

        unsafe {
            self.channel.enable();
        }
        self.in_flight = true;
    }

    fn finalize_continuous_transfer(&mut self) {
        // continuous モードを終了させる: CONT=0, CONTC=0 の TCR を書く
        unsafe {
            const LPSPI4_TCR: *mut u32 = (0x403A_0000 + 0x60) as *mut u32;
            // FRAMESZ=31, RXMSK=1, CONT=0, CONTC=0
            let tcr_val: u32 = 31 | (1 << 19);
            core::ptr::write_volatile(LPSPI4_TCR, tcr_val);
        }

        // TX FIFO が空になるまで待つ
        unsafe {
            const LPSPI4_SR: *const u32 = (0x403A_0000 + 0x14) as *const u32;
            // BUSY フラグ (bit 24) が落ちるまで待つ
            while core::ptr::read_volatile(LPSPI4_SR) & (1 << 24) != 0 {}
        }

        // DMA TX 要求を無効化 (DER.TDDE = 0)
        unsafe {
            const LPSPI4_DER: *mut u32 = (0x403A_0000 + 0x1C) as *mut u32;
            while core::ptr::read_volatile(LPSPI4_DER) & 1 != 0 {
                let der_val = core::ptr::read_volatile(LPSPI4_DER);
                core::ptr::write_volatile(LPSPI4_DER, der_val & !1);
            }
        }
    }
}

impl<P, SPI, DC, RST> Display for DmaDisplay<P, SPI, DC, RST>
where
    P: PanelController,
    SPI: SpiWrite<u8>,
    DC: OutputPin,
    RST: OutputPin,
{
    fn draw(&mut self, pixels: &[u16]) {
        // 1. 前回の DMA 完了を待つ
        self.wait_dma_complete();

        // 1.5. FPS を更新。値が変わった時 (≒毎秒1回) だけ余白に PIO 描画する。
        //      ここは GB の DMA が走っていない区間なので SPI バスは空いている。
        // 検証のため一時的に PIO 描画 (render_overlay) を無効化 (2026-07-09)。
        // 音切れ・全体的な遅延の原因がここにあるかどうかを切り分けるため。
        self.update_fps();
        if self.fps_dirty {
            self.fps_dirty = false;
        }

        // 2. バックバッファにピクセルデータを変換
        let buf = unsafe { &mut FB[self.back] };
        Self::fill_buffer(buf, pixels);

        // 3. ウィンドウ設定コマンドを PIO (ブロッキング) で送信
        self.set_window();

        // 4. DC を HIGH にしてデータモードに切り替え
        let _ = self.dc.set_high();

        // 5. バッファを入れ替えて DMA 開始
        self.back = 1 - self.back;
        self.start_dma();
    }
}
