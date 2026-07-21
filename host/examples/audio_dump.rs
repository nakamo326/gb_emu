//! APU 出力の検証用 WAV ダンプ。
//!
//! 使い方: cargo run -p gb-host --release --example audio_dump <rom.gb> [seconds] [out.wav]
//!
//! 指定秒数ぶんヘッドレス実行して音声サンプルを WAV (44100Hz/16bit/stereo) に保存し、
//! 簡易統計 (RMS / ピーク / 高域エネルギー比) を表示する。高域エネルギー比は
//! 一次差分エネルギー / 全体エネルギーで、エイリアシング低減の定量確認に使う。

use gb_core::bootrom::Bootrom;
use gb_core::gameboy::GameBoy;
use gb_core::input::NullInput;
use gb_core::mmu::Mmu;
use gb_core::platform::{AudioSink, NullDisplay};
use gb_host::cartridge::Cartridge;
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

struct CaptureAudio(Rc<RefCell<Vec<(f32, f32)>>>);

impl AudioSink for CaptureAudio {
    fn push(&mut self, left: f32, right: f32) {
        self.0.borrow_mut().push((left, right));
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let rom_path = args.next().expect("usage: audio_dump <rom.gb> [seconds] [out.wav]");
    let seconds: u64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(10);
    let out_path = args.next().unwrap_or_else(|| "audio_dump.wav".into());

    let samples = Rc::new(RefCell::new(Vec::new()));
    let cart = Cartridge::new(&rom_path).expect("failed to load ROM");
    let mmu = Mmu::new(Bootrom::disabled(), cart);
    let mut gb = GameBoy::new(mmu, NullDisplay, CaptureAudio(samples.clone()), NullInput);

    // ダブルスピード中は 1 step = 半 M-cycle なので、サンプル数ベースで回す
    let target = 44_100 * seconds as usize;
    while samples.borrow().len() < target {
        gb.step();
    }

    let samples = samples.borrow();
    let mut energy = 0f64;
    let mut diff_energy = 0f64;
    let mut peak = 0f32;
    let mut prev = 0f32;
    for &(l, r) in samples.iter() {
        assert!(l.is_finite() && r.is_finite(), "non-finite sample");
        let m = (l + r) * 0.5;
        energy += (m * m) as f64;
        let d = m - prev;
        diff_energy += (d * d) as f64;
        prev = m;
        peak = peak.max(l.abs()).max(r.abs());
    }
    let rms = (energy / samples.len() as f64).sqrt();
    println!(
        "samples: {}  rms: {:.4}  peak: {:.4}  hf-ratio (diff/total): {:.4}",
        samples.len(),
        rms,
        peak,
        diff_energy / energy.max(1e-12),
    );

    // WAV 書き出し (44100Hz / 16bit / stereo)
    let mut pcm = Vec::with_capacity(samples.len() * 4);
    for &(l, r) in samples.iter() {
        for v in [l, r] {
            let s = (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            pcm.extend_from_slice(&s.to_le_bytes());
        }
    }
    let mut f = std::fs::File::create(&out_path).unwrap();
    let data_len = pcm.len() as u32;
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
    f.write_all(&2u16.to_le_bytes()).unwrap(); // stereo
    f.write_all(&44_100u32.to_le_bytes()).unwrap();
    f.write_all(&(44_100u32 * 4).to_le_bytes()).unwrap();
    f.write_all(&4u16.to_le_bytes()).unwrap(); // block align
    f.write_all(&16u16.to_le_bytes()).unwrap(); // bits
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    f.write_all(&pcm).unwrap();
    println!("wrote {out_path}");
}
