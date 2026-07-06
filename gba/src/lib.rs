//! ゲームボーイアドバンス (AGB) エミュレーションコア。
//!
//! gb-core と同様に no_std（+alloc）で、プラットフォーム非依存。
//! ホスト側は [`gba::Gba`] を生成し、`run_frame()` → framebuffer 描画 →
//! `set_keys()` のループを回す。

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod bus;
pub mod cpu;
pub mod dma;
pub mod gba;
pub mod ppu;
pub mod timer;
