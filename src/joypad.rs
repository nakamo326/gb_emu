use crate::input::ButtonState;

pub struct Joypad {
    /// アクションボタン: [A, B, Select, Start]
    action: [bool; 4],
    /// 方向ボタン: [Right, Left, Up, Down]
    direction: [bool; 4],
    /// セレクトビット (bit5=action選択, bit4=方向選択、0=選択中)
    select: u8,
}

impl Joypad {
    pub fn new() -> Self {
        Self {
            action: [false; 4],
            direction: [false; 4],
            select: 0x30, // 両方非選択
        }
    }

    pub fn read(&self) -> u8 {
        let mut nibble = 0x0F;
        if self.select & 0x20 == 0 {
            // bit5=0: アクションボタン選択
            for (i, &pressed) in self.action.iter().enumerate() {
                if pressed {
                    nibble &= !(1 << i);
                }
            }
        }
        if self.select & 0x10 == 0 {
            // bit4=0: 方向ボタン選択
            for (i, &pressed) in self.direction.iter().enumerate() {
                if pressed {
                    nibble &= !(1 << i);
                }
            }
        }
        0xC0 | self.select | nibble
    }

    pub fn write(&mut self, val: u8) {
        self.select = val & 0x30;
    }

    /// ボタン状態を更新し、新たに押下されたボタンがあれば true を返す
    pub fn update(&mut self, s: &ButtonState) -> bool {
        let prev_action = self.action;
        let prev_direction = self.direction;
        self.action = [s.a, s.b, s.select, s.start];
        self.direction = [s.right, s.left, s.up, s.down];
        for (prev, cur) in prev_action.iter().zip(self.action.iter()) {
            if !prev && *cur {
                return true;
            }
        }
        for (prev, cur) in prev_direction.iter().zip(self.direction.iter()) {
            if !prev && *cur {
                return true;
            }
        }
        false
    }
}
