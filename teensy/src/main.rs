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
///     よって pin 7 は GPIO として空いており、今後ボタン入力 (input.rs) に転用予定。
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

    // ------- DWT サイクルカウンタ有効化 (フレームペーシングのタイマー) -------
    cp.DCB.enable_trace();
    cp.DWT.enable_cycle_counter();

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
        // BSP の set_spi_clock は分周 half_div を下限3でクランプ → SCKDIV=4 固定。
        // SPI = 132MHz/(4+2) = 約22MHz が実効上限で、ここに何を渡しても 22MHz になる。
        // 実クロックは直後の CCR 直書きで設定するため、この値は形式的なもの。
        24_000_000,
    );

    // BSP のクランプを外し、CCR を直接書き換えて SPI を高速化する。
    // SCKDIV=2 → 132MHz/(2+2) = 33MHz。これで全画面 DMA 転送が約11msに収まり、
    // フレーム予算(16.7ms)の裏に完全に隠れる (実機計測で wait=0 を確認)。
    // CCR は LPSPI 無効時のみ書けるため MEN をトグルする。DBT/PCSSCK/SCKPCS は
    // half_div=2 相当の 1 (クランプが無ければ hal が算出したはずの値と同一)。
    // continuous mode で SCK が乱れて表示が崩れる場合は SCKDIV を 4 (22MHz) に戻す。
    unsafe {
        const LPSPI4_BASE: u32 = 0x403A_0000;
        let cr = (LPSPI4_BASE + 0x10) as *mut u32; // 制御レジスタ
        let ccr = (LPSPI4_BASE + 0x40) as *mut u32; // クロック構成レジスタ
        const SCKDIV: u32 = 2; // 132/(2+2)=33MHz。22MHz に戻すなら 4
        const DLY: u32 = 1; // DBT/PCSSCK/SCKPCS (= half_div-1)

        let men = core::ptr::read_volatile(cr) & 1;
        core::ptr::write_volatile(cr, core::ptr::read_volatile(cr) & !1); // MEN=0
        core::ptr::write_volatile(
            ccr,
            (DLY << 24) | (DLY << 16) | (DLY << 8) | SCKDIV, // SCKPCS|PCSSCK|DBT|SCKDIV
        );
        core::ptr::write_volatile(cr, core::ptr::read_volatile(cr) | men); // MEN 復帰
    }

    // バックライト(BL)は 3.3V 直結のため GPIO 駆動は不要 (上のコメント参照)。
    // pin 7 はボタン入力 (input.rs) で使う予定のため、ここでは確保しない。
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

    // ------- メインループ (フレームペーシング) -------
    // GB 1 フレーム = 70224 T-cycle。ARM クロック換算のフレーム周期 (約16.742ms) ごとに
    // ループを同期させ、emu/描画がどれだけ速くても realtime (59.7fps) に固定する。
    // SPI 33MHz 化で work(emu+描画) が予算を大きく下回り、約11msの余裕が生まれている。
    use cortex_m::peripheral::DWT;
    const FRAME_CYCLES: u32 = (board::ARM_FREQUENCY as u64 * 70224 / 4_194_304) as u32;
    let mut next_deadline = DWT::cycle_count().wrapping_add(FRAME_CYCLES);

    loop {
        let r = gb.step();

        if r.frame_ready {
            let now = DWT::cycle_count();
            if (now.wrapping_sub(next_deadline) as i32) >= 0 {
                // 締切超過 (処理が予算を上回った) → 同期しなおしてバースト追い上げを防ぐ
                next_deadline = now.wrapping_add(FRAME_CYCLES);
            } else {
                // 締切まで待機して realtime に同期
                while (DWT::cycle_count().wrapping_sub(next_deadline) as i32) < 0 {}
                next_deadline = next_deadline.wrapping_add(FRAME_CYCLES);
            }
        }
    }
}
