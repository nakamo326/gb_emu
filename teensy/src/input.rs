use gb_core::input::{ButtonState, InputSource};
use teensy4_bsp as bsp;

use bsp::hal::gpio::{Input, Output, Port};
use bsp::hal::iomuxc::{configure, Config, Hysteresis, PullKeeper};
use bsp::pins::t41::{P28, P29, P30, P31, P32, P36};
use cortex_m::asm;

/// 列選択を駆動してから入力が安定するまでの待ち時間 (cycle @600MHz)。
/// 22k プルアップ + 配線容量の RC は約 1µs。余裕を見て約 5µs (≈3000 cycle)。
const SETTLE_DELAY: u32 = 3000;

/// 入力ピン共通の pad 設定: 内部 22k プルアップ + ヒステリシス有効 (チャタリング耐性)。
const IN_CFG: Config = Config::zero()
    .set_pull_keeper(Some(PullKeeper::Pullup22k))
    .set_hysteresis(Hysteresis::Enabled);

/// GPIO ボタン入力 (2x4 マトリクス, GB ジョイパッド準拠)。
///
/// # ピン割り当て (Teensy 4.1, 確定)
///
/// | 信号             | Teensy ピン | 向き  | GPIO        | 備考                         |
/// |-----------------|------------|------|-------------|------------------------------|
/// | SEL_DIR         | 28         | 出力  | GPIO3[18]   | LOW で方向キー列を選択        |
/// | SEL_ACT         | 29         | 出力  | GPIO4[31]   | LOW でアクションボタン列を選択 |
/// | IN0             | 30         | 入力  | GPIO3[23]   | SEL_DIR→右 / SEL_ACT→A       |
/// | IN1             | 31         | 入力  | GPIO3[22]   | SEL_DIR→左 / SEL_ACT→B       |
/// | IN2             | 32         | 入力  | GPIO2[12]   | SEL_DIR→上 / SEL_ACT→Select  |
/// | IN3             | 36         | 入力  | GPIO2[18]   | SEL_DIR→下 / SEL_ACT→Start   |
///
/// # 配線
///
/// - IN0-IN3 は内部 22k プルアップ + active-low (押下で LOW)。外付け抵抗は不要。
/// - 各ボタンに直列ダイオードを入れて同時押し時のゴースト/回り込みを防ぐ
///   (アノードを IN 側、カソードを SEL 側)。
/// - 走査手順: SEL_DIR=LOW/SEL_ACT=HIGH で方向 4 本を読み、次に
///   SEL_DIR=HIGH/SEL_ACT=LOW でアクション 4 本を読む。
pub struct GpioInput {
    sel_dir: Output<P28>,
    sel_act: Output<P29>,
    in0: Input<P30>,
    in1: Input<P31>,
    in2: Input<P32>,
    in3: Input<P36>,
}

impl GpioInput {
    /// マトリクス用 GPIO を構築する。
    ///
    /// ピンの GPIO ポートは型レベルで固定されている (例: pin28=EMC_32 は GPIO3) ため、
    /// 誤ったポートを渡すとコンパイルエラーになる。
    pub fn new(
        gpio2: &mut Port<2>,
        gpio3: &mut Port<3>,
        gpio4: &mut Port<4>,
        p28: P28,
        p29: P29,
        mut p30: P30,
        mut p31: P31,
        mut p32: P32,
        mut p36: P36,
    ) -> Self {
        // 入力ピンに内部プルアップ + ヒステリシスを設定する。
        // configure は input() が pin を消費する前に行う必要がある。
        configure(&mut p30, IN_CFG);
        configure(&mut p31, IN_CFG);
        configure(&mut p32, IN_CFG);
        configure(&mut p36, IN_CFG);

        let sel_dir = gpio3.output(p28);
        let sel_act = gpio4.output(p29);
        // 待機時は両列 HIGH (非選択)。
        sel_dir.set();
        sel_act.set();

        Self {
            sel_dir,
            sel_act,
            in0: gpio3.input(p30),
            in1: gpio3.input(p31),
            in2: gpio2.input(p32),
            in3: gpio2.input(p36),
        }
    }
}

impl InputSource for GpioInput {
    fn poll(&mut self) -> ButtonState {
        let mut st = ButtonState::default();

        // --- 方向キー列: SEL_DIR=LOW, SEL_ACT=HIGH ---
        self.sel_act.set();
        self.sel_dir.clear();
        asm::delay(SETTLE_DELAY);
        // active-low: 押下で LOW (is_set()==false)
        st.right = !self.in0.is_set();
        st.left = !self.in1.is_set();
        st.up = !self.in2.is_set();
        st.down = !self.in3.is_set();

        // --- アクションボタン列: SEL_ACT=LOW, SEL_DIR=HIGH ---
        self.sel_dir.set();
        self.sel_act.clear();
        asm::delay(SETTLE_DELAY);
        st.a = !self.in0.is_set();
        st.b = !self.in1.is_set();
        st.select = !self.in2.is_set();
        st.start = !self.in3.is_set();

        // 待機状態へ戻す (両列 HIGH)。
        self.sel_act.set();

        st
    }
}
