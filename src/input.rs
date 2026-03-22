#[derive(Default, Clone, Copy)]
pub struct ButtonState {
    pub a: bool,
    pub b: bool,
    pub start: bool,
    pub select: bool,
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub quit: bool,
}

pub trait InputSource {
    fn poll(&mut self) -> ButtonState;
}

pub struct NullInput;

impl InputSource for NullInput {
    fn poll(&mut self) -> ButtonState {
        ButtonState::default()
    }
}
