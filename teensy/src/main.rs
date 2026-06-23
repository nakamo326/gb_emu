#![no_std]
#![no_main]

mod audio;
mod cartridge;
mod display;
mod input;
mod sdcard;

use teensy4_bsp as bsp;
use teensy4_panic as _;

use bsp::board;

use gb_core::{
    bootrom::Bootrom,
    gameboy::GameBoy,
    mmu::Mmu,
    platform::NullAudio,
};

use display::DmaDisplay;
use display::panel::Ili9341;
use input::GpioInput;
use sdcard::FlashCart;

/// Teensy 4.1 ピン割り当て:
///
/// ILI9341 ディスプレイ (LPSPI4):
///   MOSI=11, MISO=12, SCK=13, CS=10(PCS0), DC=9, RST=8
///
/// 実機検証で判明した配線の重要事項 (詳細は docs/teensy_setup_guide.md):
///   - バックライト(LED/BL)は GPIO では電流不足で駆動不可 → 3.3V に直結する。
///     下の p7 set_high は無害だが実際のバックライト点灯には寄与しない。
///   - 単一 SPI デバイスなら CS→GND, RESET→3.3V 固定が最も確実 (その場合 p10/p8 は未使用)。
///
/// ROM は Flash に埋め込み (include_bytes!)。
/// SDカード対応は docs/teensy_setup_guide.md を参照。

// ROM は Flash に埋め込む。ビルド前に roms/game.gb を配置すること。
static ROM: &[u8] = include_bytes!("../../roms/game.gb");

#[bsp::rt::entry]
fn main() -> ! {
    let board::Resources {
        lpspi4,
        mut gpio2,
        mut dma,
        pins,
        usb,
        ..
    } = board::t41(board::instances());

    let mut cp = cortex_m::Peripherals::take().unwrap();

    // ------- L1 キャッシュ有効化 -------
    // Cortex-M7 はリセット時キャッシュ無効。ROM は Flash の XIP 配置 (build.rs) のため、
    // D-cache が無いと GB の命令フェッチごとに低速な FlexSPI アクセスが発生し激遅になる。
    // DMA 対象のフレームバッファ FB は DTCM (非キャッシュの TCM) にあるため、
    // D-cache を有効化しても DMA とのコヒーレンシ問題は生じない。
    cp.SCB.enable_icache();
    cp.SCB.enable_dcache(&mut cp.CPUID);

    // ------- 計測: DWT サイクルカウンタ有効化 -------
    cp.DCB.enable_trace();
    cp.DWT.enable_cycle_counter();

    // ------- 計測: USB シリアルログ (割り込み無効 → 手動ポーリング) -------
    let mut poller = imxrt_log::log::usbd(usb, imxrt_log::Interrupts::Disabled).unwrap();

    // ------- ROM (Flash 埋め込み) -------
    let cart = FlashCart::new(ROM);

    // ------- ILI9341 ディスプレイ (LPSPI4) -------
    let spi: board::Lpspi4 = board::lpspi(
        lpspi4,
        board::LpspiPins {
            sdo: pins.p11,
            sdi: pins.p12,
            sck: pins.p13,
            pcs0: pins.p10,
        },
        24_000_000, // 描画高速化のため 24MHz。配線が崩れる場合は下げる
    );

    // バックライト有効化
    let mut bl = gpio2.output(pins.p7);
    let _ = embedded_hal::digital::v2::OutputPin::set_high(&mut bl);

    let dc  = gpio2.output(pins.p9);
    let rst = gpio2.output(pins.p8);
    let dma_channel = dma[0].take().unwrap();
    let display = DmaDisplay::<Ili9341, _, _, _>::new(spi, dc, rst, dma_channel);

    // ------- GB コア -------

    // BootROM を使う場合: Bootrom::from_bytes(*include_bytes!("../dmg_bootrom.bin"))
    // 著作権注意 — 配布不可
    let bootrom = Bootrom::disabled();

    let input = GpioInput::new();
    let mmu = Mmu::new(bootrom, cart);
    let mut gb = GameBoy::new(mmu, display, NullAudio, input);

    // ------- メインループ (計測付き) -------
    use cortex_m::peripheral::DWT;
    use display::{DRAW_CYCLES, WAIT_CYCLES};
    use core::sync::atomic::Ordering;

    // 600MHz → 1µs あたりのサイクル数
    const CYC_PER_US: u64 = (board::ARM_FREQUENCY / 1_000_000) as u64;
    const REPORT_FRAMES: u32 = 60;

    let mut last = DWT::cycle_count();
    let mut sum_period: u64 = 0;
    let mut sum_draw: u64 = 0;
    let mut sum_wait: u64 = 0;
    let mut frames: u32 = 0;
    let mut step_ctr: u32 = 0;

    loop {
        let r = gb.step();

        // USB ログを定期的に駆動 (割り込み無効のため手動ポーリング)。
        // 毎 M-cycle だと計測を歪めるので 256 サイクルごとに間引く。
        step_ctr = step_ctr.wrapping_add(1);
        if step_ctr & 0xFF == 0 {
            poller.poll();
        }

        if r.frame_ready {
            let now = DWT::cycle_count();
            sum_period += now.wrapping_sub(last) as u64;
            sum_draw += DRAW_CYCLES.load(Ordering::Relaxed) as u64;
            sum_wait += WAIT_CYCLES.load(Ordering::Relaxed) as u64;
            last = now;
            frames += 1;

            if frames >= REPORT_FRAMES {
                let n = frames as u64;
                let period_us = sum_period / n / CYC_PER_US;
                let draw_us = sum_draw / n / CYC_PER_US;
                let wait_us = sum_wait / n / CYC_PER_US;
                let emu_us = (sum_period - sum_draw) / n / CYC_PER_US;
                // fps = ARM_FREQUENCY / 平均周期。小数1桁まで出す。
                let fps_x10 = board::ARM_FREQUENCY as u64 * 10 * n / sum_period;

                log::info!(
                    "fps={}.{} period={}us emu={}us draw={}us wait={}us",
                    fps_x10 / 10,
                    fps_x10 % 10,
                    period_us,
                    emu_us,
                    draw_us,
                    wait_us,
                );

                frames = 0;
                sum_period = 0;
                sum_draw = 0;
                sum_wait = 0;
            }
        }
    }
}
