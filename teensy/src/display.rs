use gb_core::platform::Display;

/// ILI9341 SPI TFT ディスプレイ (240×320) への出力。
/// GB の 160×144 バッファを中央に表示する。
/// TODO: SPI + DC/CS/RST ピン制御を実装する（タスク C）。
pub struct Ili9341Display;

impl Ili9341Display {
    pub fn new() -> Self {
        Self
    }
}

impl Display for Ili9341Display {
    fn draw(&mut self, _pixels: &[u8]) {
        // TODO: パレット変換 + ILI9341 フレーム転送
    }
}
