pub mod panel;

use core::marker::PhantomData;

use gb_core::platform::Display;

use embedded_hal::blocking::spi::Write as SpiWrite;
use embedded_hal::digital::v2::OutputPin;

use teensy4_bsp as bsp;
use bsp::hal;
use hal::dma::channel::{self, Channel, Configuration};
use panel::PanelController;

const GB_W: usize = 160;
const GB_H: usize = 144;
const PIXEL_COUNT: usize = GB_W * GB_H; // 23040
const BUF_U32: usize = PIXEL_COUNT / 2; // 11520 — 2px を 1 u32 にパック

const CASET: u8 = 0x2A;
const RASET: u8 = 0x2B;
const RAMWR: u8 = 0x2C;

const SPI_MAX_CHUNK: usize = 512;

const PALETTE: [u16; 4] = [
    rgb565(0xE0, 0xF8, 0xD0),
    rgb565(0x88, 0xC0, 0x70),
    rgb565(0x34, 0x68, 0x56),
    rgb565(0x0E, 0x18, 0x20),
];

const fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | ((b as u16) >> 3)
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

    fn set_window(&mut self) {
        let x0 = (P::WIDTH - GB_W as u16) / 2 + P::COL_OFFSET;
        let y0 = (P::HEIGHT - GB_H as u16) / 2 + P::ROW_OFFSET;
        let x1 = x0 + GB_W as u16 - 1;
        let y1 = y0 + GB_H as u16 - 1;

        self.cmd(CASET);
        self.data_pio(&[(x0 >> 8) as u8, x0 as u8, (x1 >> 8) as u8, x1 as u8]);
        self.cmd(RASET);
        self.data_pio(&[(y0 >> 8) as u8, y0 as u8, (y1 >> 8) as u8, y1 as u8]);
        self.cmd(RAMWR);
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

    fn fill_buffer(buf: &mut [u32; BUF_U32], pixels: &[u8]) {
        let len = pixels.len().min(PIXEL_COUNT);
        let pairs = len / 2;
        for i in 0..pairs {
            let c0 = PALETTE[(pixels[i * 2] & 3) as usize];
            let c1 = PALETTE[(pixels[i * 2 + 1] & 3) as usize];
            buf[i] = (c0 as u32) << 16 | c1 as u32;
        }
        if len & 1 != 0 {
            let c = PALETTE[(pixels[len - 1] & 3) as usize];
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
                | (1 << 21);      // CONT
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
    fn draw(&mut self, pixels: &[u8]) {
        // 1. 前回の DMA 完了を待つ
        self.wait_dma_complete();

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
