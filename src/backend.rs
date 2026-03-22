use crate::input::ButtonState;

/// 表示と入力を一体化したバックエンドトレイト
pub trait Backend {
    fn draw(&mut self, buffer: &[u8]);
    fn poll(&mut self) -> ButtonState;
    fn push_audio(&mut self, samples: &[f32]);
}

/// テスト・ヘッドレス用 no-op 実装
pub struct NullBackend;

impl Backend for NullBackend {
    fn draw(&mut self, _: &[u8]) {}
    fn poll(&mut self) -> ButtonState {
        ButtonState::default()
    }
    fn push_audio(&mut self, _: &[f32]) {}
}
