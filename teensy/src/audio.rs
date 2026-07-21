/// I2S オーディオ出力 (MAX98357A / PCM5102A 向け)。
///
/// # ピン割り当て (Teensy 4.1, SAI1 TX)
///
/// | 信号          | Teensy ピン | パッド         | 備考                        |
/// |--------------|------------|---------------|-----------------------------|
/// | SAI1_TX_DATA | 7          | GPIO_B1_01    | MAX98357A DIN               |
/// | SAI1_TX_BCLK | 26         | GPIO_AD_B1_14 | MAX98357A BCLK              |
/// | SAI1_TX_SYNC | 27         | GPIO_AD_B1_15 | MAX98357A LRC               |
/// | SAI1_MCLK    | 23         | GPIO_AD_B1_09 | 未接続 (API 要件のため使用) |
///
/// # クロック
///
/// BSP が PLL4 を設定済み: SAI1_FREQUENCY ≈ 11,293,920 Hz
/// bclk_div(8) → BCLK = 11,293,920 / 8 ≈ 1,411,740 Hz ≈ 44100 × 32 ✓
///
/// # アーキテクチャ
///
/// `push()` はリングバッファ (1024 フレーム ≈ 23ms) に書き込む。
/// SAI1 割り込みが FIFO が枯渇するたびにリングバッファから FIFO を補充する。
///
/// # ペーシング (オーディオマスター同期)
///
/// APU の生成レート (実時間換算 44100 Hz) と SAI の実消費レート
/// (MCLK/8/32 ≈ 44116.9 Hz) は一致しないため、ドロップ方式ではバッファが
/// 必ず空近傍の平衡点に落ち、わずかなジッタが毎回無音ギャップになる。
/// そこで `push()` はバッファ満杯時に 1 スロット空くまでスピン待機する。
/// これでエミュレーション全体が DAC の実レートにロックされ、バッファは
/// 常時ほぼ満杯 (≈23ms のヘッドルーム) を維持する。main ループの DWT 締切は
/// セーフティキャップとしてのみ機能する (main.rs 参照)。
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering};

use cortex_m::peripheral::DWT;

use teensy4_bsp as bsp;
use bsp::hal::iomuxc::{
    consts::U1,
    sai as iomuxc_sai,
};
use bsp::hal::sai::{bclk_div, Interrupts, PackingNone, Pins as SaiPins, Sai, SaiConfig, Status, Tx};
use bsp::ral;

use gb_core::platform::AudioSink;

type SaiTx = Tx<1, 16, 2, PackingNone>;

// --- SAI TX (main で初期化後は割り込みハンドラのみが使用) ---
struct SaiCell(UnsafeCell<Option<SaiTx>>);
unsafe impl Sync for SaiCell {}
static SAI_TX: SaiCell = SaiCell(UnsafeCell::new(None));

// --- SPSC リングバッファ (producer = push, consumer = SAI1 割り込み) ---
const RING_SIZE: usize = 1024; // 2 の累乗

struct RingBuf {
    data: UnsafeCell<[u16; RING_SIZE * 2]>, // L/R インターリーブ
    head: AtomicU16,                         // producer が進める
    tail: AtomicU16,                         // consumer が進める
}
unsafe impl Sync for RingBuf {}

static RING: RingBuf = RingBuf {
    data: UnsafeCell::new([0u16; RING_SIZE * 2]),
    // head を RING_SIZE から開始し、全スロットをサイレンス済み扱いで「満杯」から始める。
    // 起動直後からペーシングが効き、バッファ充填中のアンダーラン (プチプチ音) を避ける。
    head: AtomicU16::new(RING_SIZE as u16),
    tail: AtomicU16::new(0),
};

// push() がペーシング待ちに費やした累積サイクル (フレーム負荷計測から除外するため)。
static BLOCKED_CYCLES: AtomicU32 = AtomicU32::new(0);
// SAI 停止検出後 true: 以降 push() は待たずにドロップする (エミュ全体のハング防止)。
static PACING_DISABLED: AtomicBool = AtomicBool::new(false);
// consumer は約 22.7µs ごとに 1 スロット空ける。1ms 待って空かなければ SAI 停止とみなす。
const STALL_TIMEOUT_CYCLES: u32 = bsp::board::ARM_FREQUENCY / 1000;

// 出力音量 (0.0-1.0)。
const VOLUME: f32 = 0.2;

pub struct SaiAudio;

impl SaiAudio {
    /// SAI1 TX を初期化して `SaiAudio` を返す。
    ///
    /// - `p7`  : GPIO_B1_01   → SAI1_TX_DATA
    /// - `p23` : GPIO_AD_B1_09 → SAI1_MCLK (MAX98357A 未使用、API 要件)
    /// - `p26` : GPIO_AD_B1_14 → SAI1_TX_BCLK
    /// - `p27` : GPIO_AD_B1_15 → SAI1_TX_SYNC
    ///
    /// 呼び出し側で `NVIC::unmask(bsp::interrupt::SAI1)` を行うこと。
    pub fn new(
        sai1: ral::sai::SAI1,
        p7: impl iomuxc_sai::Pin<U1, Signal = iomuxc_sai::TxData>,
        p23: impl iomuxc_sai::Pin<U1, Signal = iomuxc_sai::Mclk>,
        p26: impl iomuxc_sai::Pin<U1, Signal = iomuxc_sai::TxBclk>,
        p27: impl iomuxc_sai::Pin<U1, Signal = iomuxc_sai::TxSync>,
    ) -> Self {
        // from_tx() が内部で sai::prepare() を呼び出してピンの IOMUXC を設定する。
        let sai = Sai::from_tx(
            sai1,
            p23,
            SaiPins { sync: p27, bclk: p26, data: p7 },
        );

        // 16-bit ステレオ、44100 Hz、bclk_div(8) で分割
        let (Some(mut tx), _) = sai.split::<16, 2, PackingNone>(&SaiConfig::i2s(bclk_div(8)))
        else {
            panic!("SAI1 TX init failed");
        };

        // 起動直後のアンダーランを防ぐため FIFO をサイレンスで事前充填
        for _ in 0..16 {
            tx.write_frame(0, [0u16, 0u16]);
        }

        // FIFO が枯渇しかけたら割り込みで補充する
        tx.set_interrupts(Interrupts::FIFO_REQUEST);
        tx.set_enable(true);

        unsafe { *SAI_TX.0.get() = Some(tx) };

        SaiAudio
    }
}

impl SaiAudio {
    /// 直前の呼び出し以降に `push()` がペーシング待ちに費やしたサイクル数を返し 0 に戻す。
    /// main ループがフレーム負荷計測 (record_work) から待機時間を除外するために使う。
    pub fn take_blocked_cycles() -> u32 {
        BLOCKED_CYCLES.swap(0, Ordering::Relaxed)
    }
}

impl AudioSink for SaiAudio {
    /// APU から呼ばれる (44100 Hz)。サンプルをリングバッファに積む。
    ///
    /// バッファ満杯時は 1 スロット空くまでスピン待機し、エミュレーションを
    /// SAI の実サンプルレートに同期させる (モジュール冒頭のコメント参照)。
    fn push(&mut self, left: f32, right: f32) {
        let head = RING.head.load(Ordering::Relaxed);
        let mut tail = RING.tail.load(Ordering::Acquire);

        if head.wrapping_sub(tail) as usize >= RING_SIZE {
            if PACING_DISABLED.load(Ordering::Relaxed) {
                return; // フェイルセーフモード: 従来どおりドロップ
            }
            let start = DWT::cycle_count();
            loop {
                tail = RING.tail.load(Ordering::Acquire);
                if (head.wrapping_sub(tail) as usize) < RING_SIZE {
                    break;
                }
                if DWT::cycle_count().wrapping_sub(start) > STALL_TIMEOUT_CYCLES {
                    PACING_DISABLED.store(true, Ordering::Relaxed);
                    log::warn!("SAI1 stalled: audio pacing disabled");
                    return;
                }
            }
            BLOCKED_CYCLES.fetch_add(
                DWT::cycle_count().wrapping_sub(start),
                Ordering::Relaxed,
            );
        }

        let idx = (head as usize & (RING_SIZE - 1)) * 2;
        let l = ((left * VOLUME).clamp(-1.0, 1.0) * i16::MAX as f32) as i16 as u16;
        let r = ((right * VOLUME).clamp(-1.0, 1.0) * i16::MAX as f32) as i16 as u16;
        unsafe {
            let buf = &mut *RING.data.get();
            buf[idx] = l;
            buf[idx + 1] = r;
        }
        RING.head.store(head.wrapping_add(1), Ordering::Release);
    }
}

/// SAI1 割り込みハンドラ本体。`main.rs` の `#[bsp::rt::interrupt] fn SAI1()` から呼ぶ。
///
/// FIFO_REQUEST フラグが立っている間、リングバッファから SAI FIFO に補充する。
/// リングバッファが空の場合はサイレンスで補充し、アンダーランを防ぐ。
pub(crate) fn on_sai1_interrupt() {
    let tx = unsafe {
        let Some(tx) = (*SAI_TX.0.get()).as_mut() else { return };
        tx
    };

    tx.clear_status(Status::FIFO_ERROR);

    while tx.status().contains(Status::FIFO_REQUEST) {
        let head = RING.head.load(Ordering::Acquire);
        let tail = RING.tail.load(Ordering::Relaxed);

        let frame = if tail != head {
            let idx = (tail as usize & (RING_SIZE - 1)) * 2;
            let f = unsafe {
                let buf = &*RING.data.get();
                [buf[idx], buf[idx + 1]]
            };
            RING.tail.store(tail.wrapping_add(1), Ordering::Release);
            f
        } else {
            // バッファ空 → サイレンスで埋める
            [0u16, 0u16]
        };

        tx.write_frame(0, frame);
    }
}
