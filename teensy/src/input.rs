use gb_core::input::{ButtonState, InputSource};

/// GPIO ボタン入力。
/// TODO: 各ボタンに割り当てたピンを読んで ButtonState を返す。
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
