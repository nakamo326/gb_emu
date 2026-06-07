//! プラットフォーム抽象トレイト群（表示・音声・カートリッジバス）。
//! 入力は [`crate::input::InputSource`] を使う。

/// 160x144 のパレットインデックス(0–3)バッファを表示する。
pub trait Display {
    fn draw(&mut self, buffer: &[u8]);
}

/// ステレオ f32 サンプルを出力先へ渡す。
pub trait AudioSink {
    fn push(&mut self, left: f32, right: f32);
}

/// カートリッジ（ROM/外部RAM/MBC）へのバスアクセス抽象。
///
/// host はファイル ROM + MBC エミュレーション、teensy は実カートの
/// GPIO バス駆動でこれを実装する。0x0000–0x7FFF（ROM/MBC レジスタ）と
/// 0xA000–0xBFFF（外部 RAM）のアドレスが渡される。
pub trait CartridgeBus {
    fn read(&self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);
}

/// 表示を破棄する no-op 実装（ヘッドレス/テスト用）。
pub struct NullDisplay;

impl Display for NullDisplay {
    fn draw(&mut self, _: &[u8]) {}
}

/// 音声を破棄する no-op 実装。
pub struct NullAudio;

impl AudioSink for NullAudio {
    fn push(&mut self, _: f32, _: f32) {}
}

/// カート未装着（読み出しは常に 0xFF）。
pub struct NullCartridge;

impl CartridgeBus for NullCartridge {
    fn read(&self, _: u16) -> u8 {
        0xFF
    }
    fn write(&mut self, _: u16, _: u8) {}
}
