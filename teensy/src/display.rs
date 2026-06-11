use gb_core::platform::Display;

use embedded_hal::blocking::spi::Write as SpiWrite;
use embedded_hal::digital::v2::OutputPin;

const GB_W: u16 = 160;
const GB_H: u16 = 144;

// ILI9341 解像度 240×320 の中央に GB 画像を配置
const X_START: u16 = (240 - GB_W) / 2;
const Y_START: u16 = (320 - GB_H) / 2;
const X_END:   u16 = X_START + GB_W - 1;
const Y_END:   u16 = Y_START + GB_H - 1;

// DMG 風グリーンパレット (RGB565)
const PALETTE: [u16; 4] = [
    rgb565(155, 188,  15), // 0: 最も明るい
    rgb565(139, 172,  15), // 1
    rgb565( 48,  98,  48), // 2
    rgb565( 15,  56,  15), // 3: 最も暗い
];

const fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | ((b as u16) >> 3)
}

// ILI9341 コマンド
const SWRESET:   u8 = 0x01;
const SLPOUT:    u8 = 0x11;
const COLMOD:    u8 = 0x3A;
const MADCTL:    u8 = 0x36;
const CASET:     u8 = 0x2A;
const PASET:     u8 = 0x2B;
const RAMWR:     u8 = 0x2C;
const DISPON:    u8 = 0x29;
const PWCTRL1:   u8 = 0xC0;
const PWCTRL2:   u8 = 0xC1;
const VMCTRL1:   u8 = 0xC5;
const VMCTRL2:   u8 = 0xC7;
const FRMCTR1:   u8 = 0xB1;
const DFUNCTR:   u8 = 0xB6;
const GAMSET:    u8 = 0x26;
const PVGAMCTRL: u8 = 0xE0;
const NVGAMCTRL: u8 = 0xE1;

/// ILI9341 SPI ディスプレイドライバ。
/// SPI は embedded-hal 0.2 `blocking::spi::Write<u8>` を実装している必要がある。
/// DC / RST は同 0.2 `digital::v2::OutputPin` を実装している必要がある。
pub struct Ili9341Display<SPI, DC, RST> {
    spi: SPI,
    dc: DC,
    rst: RST,
    // 160×144 × 2 bytes = 46080 bytes のフレームバッファ (DTCM 上)
    buf: [u8; (GB_W as usize) * (GB_H as usize) * 2],
}

impl<SPI, DC, RST> Ili9341Display<SPI, DC, RST>
where
    SPI: SpiWrite<u8>,
    DC: OutputPin,
    RST: OutputPin,
{
    /// SPI、DC ピン、RST ピンを受け取り ILI9341 を初期化する。
    /// `spi` には `board::lpspi(...)` の戻り値を渡す。
    /// `dc`・`rst` には `gpio2.output(pin)` の戻り値を渡す。
    pub fn new(spi: SPI, dc: DC, rst: RST) -> Self {
        let mut d = Self {
            spi,
            dc,
            rst,
            buf: [0u8; (GB_W as usize) * (GB_H as usize) * 2],
        };
        d.init();
        d
    }

    // Cortex-M7 600 MHz 想定。delay() は約 3 サイクル/イテレーション。
    fn delay_ms(ms: u32) {
        cortex_m::asm::delay(teensy4_bsp::board::ARM_FREQUENCY / 1_000 / 3 * ms);
    }

    fn cmd(&mut self, c: u8) {
        let _ = self.dc.set_low();
        let _ = self.spi.write(&[c]);
    }

    fn data(&mut self, d: &[u8]) {
        let _ = self.dc.set_high();
        let _ = self.spi.write(d);
    }

    fn set_window(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) {
        self.cmd(CASET);
        self.data(&[(x0 >> 8) as u8, x0 as u8, (x1 >> 8) as u8, x1 as u8]);
        self.cmd(PASET);
        self.data(&[(y0 >> 8) as u8, y0 as u8, (y1 >> 8) as u8, y1 as u8]);
        self.cmd(RAMWR);
    }

    fn init(&mut self) {
        // ハードウェアリセット
        let _ = self.rst.set_low();
        Self::delay_ms(10);
        let _ = self.rst.set_high();
        Self::delay_ms(120);

        // ソフトウェアリセット
        self.cmd(SWRESET);
        Self::delay_ms(120);

        // スリープ解除 — ILI9341 データシート: SLPOUT 後 DISPON まで 120ms 必要
        self.cmd(SLPOUT);
        Self::delay_ms(150);

        // 電源設定
        self.cmd(PWCTRL1); self.data(&[0x23]);
        self.cmd(PWCTRL2); self.data(&[0x10]);
        self.cmd(VMCTRL1); self.data(&[0x3E, 0x28]);
        self.cmd(VMCTRL2); self.data(&[0x86]);

        // メモリアクセス制御: ポートレート, RGB 順
        self.cmd(MADCTL); self.data(&[0x48]);

        // ピクセルフォーマット: 16-bit RGB565
        self.cmd(COLMOD); self.data(&[0x55]);

        // フレームレート設定
        self.cmd(FRMCTR1); self.data(&[0x00, 0x18]);

        // ディスプレイファンクション制御
        self.cmd(DFUNCTR); self.data(&[0x08, 0x82, 0x27]);

        // ガンマ設定
        self.cmd(GAMSET); self.data(&[0x01]);
        self.cmd(PVGAMCTRL);
        self.data(&[0x0F,0x31,0x2B,0x0C,0x0E,0x08,0x4E,0xF1,
                    0x37,0x07,0x10,0x03,0x0E,0x09,0x00]);
        self.cmd(NVGAMCTRL);
        self.data(&[0x00,0x0E,0x14,0x03,0x11,0x07,0x31,0xC1,
                    0x48,0x08,0x0F,0x0C,0x31,0x36,0x0F]);

        // ディスプレイ ON
        self.cmd(DISPON);
        Self::delay_ms(10);
    }
}

impl<SPI, DC, RST> Display for Ili9341Display<SPI, DC, RST>
where
    SPI: SpiWrite<u8>,
    DC: OutputPin,
    RST: OutputPin,
{
    fn draw(&mut self, pixels: &[u8]) {
        // パレットインデックス → RGB565 に変換してフレームバッファに書き込む
        let len = pixels.len().min((GB_W as usize) * (GB_H as usize));
        for i in 0..len {
            let color = PALETTE[(pixels[i] & 3) as usize];
            self.buf[i * 2]     = (color >> 8) as u8;
            self.buf[i * 2 + 1] = color as u8;
        }

        // ウィンドウを設定して 1 回の SPI 転送でフレーム全体を書き込む
        self.set_window(X_START, Y_START, X_END, Y_END);
        let _ = self.dc.set_high();
        let buf_len = len * 2;
        let _ = self.spi.write(&self.buf[..buf_len]);
    }
}
