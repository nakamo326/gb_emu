// work ram of 8KiB
pub struct WRam {
    val: [u8; 0x2000],
}

impl WRam {
    pub fn new() -> Self {
        Self { val: [0; 0x2000] }
    }

    pub fn read(&self, addr: u16) -> u8 {
        self.val[(addr as usize) & 0x1fff]
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        self.val[(addr as usize) & 0x1fff] = val;
    }
}
