pub trait Renderer {
    fn draw(&mut self, pixel_buffer: &[u8]);
}

pub struct TerminalRenderer {
    width: usize,
    height: usize,
}

impl TerminalRenderer {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height }
    }

    // ピクセル値をASCII文字に変換する
    fn pixel_to_ascii(&self, pixel: u8) -> char {
        match pixel {
            0 => ' ', // 一番暗い（白っぽい）
            1 => '░', // 薄いグレー
            2 => '▒', // 中くらいのグレー
            _ => '█', // 一番濃い（黒っぽい）
        }
    }

    // ターミナルをクリアする
    fn clear_screen(&self) {
        print!("\x1B[2J\x1B[H");
    }
}

impl Renderer for TerminalRenderer {
    fn draw(&mut self, pixel_buffer: &[u8]) {
        self.clear_screen();

        // ピクセルバッファを1行ずつ処理
        for y in 0..self.height {
            for x in 0..self.width {
                let index = y * self.width + x;
                if index < pixel_buffer.len() {
                    let ascii_char = self.pixel_to_ascii(pixel_buffer[index]);
                    print!("{}", ascii_char);
                } else {
                    print!(" "); // バッファが足りない場合は空白
                }
            }
            println!(); // 改行
        }

        // バッファをフラッシュして即座に表示
        use std::io::{self, Write};
        io::stdout().flush().unwrap();
    }
}
