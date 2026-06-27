pub trait Renderer {
    fn draw(&mut self, pixel_buffer: &[u8]);
}

pub struct NullRenderer;

impl Renderer for NullRenderer {
    fn draw(&mut self, _: &[u8]) {}
}

pub struct TerminalRenderer {
    width: usize,
    height: usize,
}

impl TerminalRenderer {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height }
    }

    fn pixel_to_ascii(&self, pixel: u8) -> char {
        // GB パレットインデックスは 0 が最も明るい。濃い文字ほど大きい値に対応させる。
        match pixel {
            0 => ' ',
            1 => '░',
            2 => '▒',
            _ => '█',
        }
    }

    fn clear_screen(&self) {
        print!("\x1B[2J\x1B[H");
    }
}

impl Renderer for TerminalRenderer {
    fn draw(&mut self, pixel_buffer: &[u8]) {
        self.clear_screen();

        for y in 0..self.height {
            for x in 0..self.width {
                let index = y * self.width + x;
                if index < pixel_buffer.len() {
                    let ascii_char = self.pixel_to_ascii(pixel_buffer[index]);
                    print!("{}", ascii_char);
                } else {
                    print!(" ");
                }
            }
            println!();
        }

        use std::io::{self, Write};
        io::stdout().flush().unwrap();
    }
}
