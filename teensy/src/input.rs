use gb_core::input::{ButtonState, InputSource};

/// GPIO ボタン入力 (2x4 マトリクス, GB ジョイパッド準拠)。
///
/// # ピン割り当て (Teensy 4.1, 確定)
///
/// | 信号             | Teensy ピン | 向き  | 備考                         |
/// |-----------------|------------|------|------------------------------|
/// | SEL_DIR         | 28         | 出力  | LOW で方向キー列を選択        |
/// | SEL_ACT         | 29         | 出力  | LOW でアクションボタン列を選択 |
/// | IN0             | 30         | 入力  | SEL_DIR→右 / SEL_ACT→A       |
/// | IN1             | 31         | 入力  | SEL_DIR→左 / SEL_ACT→B       |
/// | IN2             | 32         | 入力  | SEL_DIR→上 / SEL_ACT→Select  |
/// | IN3             | 36         | 入力  | SEL_DIR→下 / SEL_ACT→Start   |
///
/// # 配線
///
/// - IN0-IN3 は内部プルアップ + active-low (押下で LOW)。
/// - 各ボタンに直列ダイオードを入れて同時押し時のゴースト/回り込みを防ぐ。
/// - 走査手順: SEL_DIR=LOW/SEL_ACT=HIGH で方向 4 本を読み、次に
///   SEL_DIR=HIGH/SEL_ACT=LOW でアクション 4 本を読む。
///
/// TODO: 各ピンを GPIO 設定し、上記走査で PSR を読んで ButtonState に変換する。
pub struct GpioInput {
    _priv: (),
}

impl GpioInput {
    pub fn new() -> Self {
        Self { _priv: () }
    }
}

impl InputSource for GpioInput {
    fn poll(&mut self) -> ButtonState {
        ButtonState::default()
    }
}
