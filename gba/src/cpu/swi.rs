//! HLE BIOS: 実 BIOS 非搭載時の SWI 実装。
//!
//! 主要な関数のみ実装し、未実装の SWI は no-op（Sound 系、Huffman、
//! Diff フィルタ等）。IntrWait 系は BIOS 割り込みフラグ (0x03007FF8) の
//! 規約に従う: ユーザーハンドラがフラグを立て、ここで照合・クリアする。

use super::Cpu;
use crate::bus::Bus;
use alloc::vec::Vec;
use core::f64::consts::{PI, TAU};

/// BIOS 割り込みフラグのミラーアドレス（IWRAM 終端 - 8）
pub const BIOS_IRQ_FLAGS: u32 = 0x0300_7FF8;

pub(super) fn execute(cpu: &mut Cpu, bus: &mut Bus, num: u32) {
    match num {
        0x00 => soft_reset(cpu, bus),
        0x01 => register_ram_reset(cpu, bus),
        0x02 | 0x03 => cpu.halted = true, // Halt / Stop
        0x04 => intr_wait(cpu, bus, cpu.regs[0] != 0, cpu.regs[1] as u16),
        0x05 => intr_wait(cpu, bus, true, 1), // VBlankIntrWait
        0x06 => div(cpu, cpu.regs[0] as i32, cpu.regs[1] as i32),
        0x07 => div(cpu, cpu.regs[1] as i32, cpu.regs[0] as i32), // DivArm
        0x08 => cpu.regs[0] = isqrt(cpu.regs[0]),
        0x09 => {
            // ArcTan: 入力 1.14 固定小数点、出力 ±0x4000 = ±π/2
            let t = cpu.regs[0] as i16 as f64 / 16384.0;
            cpu.regs[0] = (libm::atan(t) / PI * 32768.0) as i32 as u32 & 0xFFFF;
        }
        0x0A => {
            // ArcTan2: 出力 0..0xFFFF = 0..2π
            let x = cpu.regs[0] as i16 as f64;
            let y = cpu.regs[1] as i16 as f64;
            let theta = libm::atan2(y, x);
            cpu.regs[0] = ((theta / TAU * 65536.0) as i32 as u32) & 0xFFFF;
        }
        0x0B => cpu_set(cpu, bus),
        0x0C => cpu_fast_set(cpu, bus),
        0x0D => cpu.regs[0] = 0xBAAE_187F, // GetBiosChecksum (AGB の既知値)
        0x0E => bg_affine_set(cpu, bus),
        0x0F => obj_affine_set(cpu, bus),
        0x11 => lz77(cpu, bus),
        0x12 => lz77(cpu, bus),
        0x14 => rl_uncomp(cpu, bus),
        0x15 => rl_uncomp(cpu, bus),
        _ => {}
    }
}

fn soft_reset(cpu: &mut Cpu, bus: &mut Bus) {
    // リターンアドレスフラグ (0x03007FFA): 0 なら ROM、それ以外は EWRAM へ
    let flag = bus.read8(0x0300_7FFA);
    for a in (0x0300_7E00..0x0300_8000u32).step_by(4) {
        bus.write32(a, 0);
    }
    cpu.apply_post_bios_state();
    for r in 0..13 {
        cpu.regs[r] = 0;
    }
    cpu.regs[14] = 0;
    cpu.regs[15] = if flag == 0 { 0x0800_0000 } else { 0x0200_0000 };
}

fn register_ram_reset(cpu: &mut Cpu, bus: &mut Bus) {
    let flags = cpu.regs[0];
    if flags & 0x01 != 0 {
        bus.ewram_mut().fill(0);
    }
    if flags & 0x02 != 0 {
        // IWRAM はスタック等が使う終端 0x200 バイトを残してクリア
        let iwram = bus.iwram_mut();
        let end = iwram.len() - 0x200;
        iwram[..end].fill(0);
    }
    if flags & 0x04 != 0 {
        bus.ppu.palette.fill(0);
    }
    if flags & 0x08 != 0 {
        bus.ppu.vram.fill(0);
    }
    if flags & 0x10 != 0 {
        bus.ppu.oam.fill(0);
    }
    // SIO / サウンド / その他 I/O のリセット (bit5-7) は未対応
}

fn intr_wait(cpu: &mut Cpu, bus: &mut Bus, discard_old: bool, mask: u16) {
    // BIOS 実装と同じく IME を強制的に有効化する
    bus.ime = true;
    let flags = bus.read16(BIOS_IRQ_FLAGS);
    if !discard_old && flags & mask != 0 {
        bus.write16(BIOS_IRQ_FLAGS, flags & !mask);
        return;
    }
    if discard_old {
        bus.write16(BIOS_IRQ_FLAGS, flags & !mask);
    }
    cpu.intr_wait_mask = Some(mask);
    cpu.halted = true;
}

fn div(cpu: &mut Cpu, num: i32, den: i32) {
    if den == 0 {
        // 実 BIOS はハング相当。ゼロ除算 panic を避けるだけの安全弁
        cpu.regs[0] = if num < 0 { u32::MAX } else { 1 };
        cpu.regs[1] = num as u32;
        cpu.regs[3] = 1;
        return;
    }
    let q = num.wrapping_div(den);
    cpu.regs[0] = q as u32;
    cpu.regs[1] = num.wrapping_rem(den) as u32;
    cpu.regs[3] = q.wrapping_abs() as u32;
}

fn isqrt(x: u32) -> u32 {
    let mut r = libm::sqrt(x as f64) as u32;
    while (r as u64) * (r as u64) > x as u64 {
        r -= 1;
    }
    while ((r + 1) as u64) * ((r + 1) as u64) <= x as u64 {
        r += 1;
    }
    r
}

fn cpu_set(cpu: &mut Cpu, bus: &mut Bus) {
    let mut src = cpu.regs[0];
    let mut dst = cpu.regs[1];
    let cnt = cpu.regs[2];
    let count = cnt & 0x1F_FFFF;
    let fill = cnt & 1 << 24 != 0;
    if cnt & 1 << 26 != 0 {
        let fill_val = bus.read32(src);
        for _ in 0..count {
            let v = if fill {
                fill_val
            } else {
                let v = bus.read32(src);
                src = src.wrapping_add(4);
                v
            };
            bus.write32(dst, v);
            dst = dst.wrapping_add(4);
        }
    } else {
        let fill_val = bus.read16(src);
        for _ in 0..count {
            let v = if fill {
                fill_val
            } else {
                let v = bus.read16(src);
                src = src.wrapping_add(2);
                v
            };
            bus.write16(dst, v);
            dst = dst.wrapping_add(2);
        }
    }
}

fn cpu_fast_set(cpu: &mut Cpu, bus: &mut Bus) {
    let mut src = cpu.regs[0];
    let mut dst = cpu.regs[1];
    let cnt = cpu.regs[2];
    // 常に 32bit、8 ワード単位に切り上げ
    let count = (cnt & 0x1F_FFFF).next_multiple_of(8);
    let fill = cnt & 1 << 24 != 0;
    let fill_val = bus.read32(src);
    for _ in 0..count {
        let v = if fill {
            fill_val
        } else {
            let v = bus.read32(src);
            src = src.wrapping_add(4);
            v
        };
        bus.write32(dst, v);
        dst = dst.wrapping_add(4);
    }
}

/// BgAffineSet: 回転拡縮パラメータを計算する。
/// pa=sx·cosθ, pb=-sx·sinθ, pc=sy·sinθ, pd=sy·cosθ、
/// 始点 = テクスチャ原点 - 行列×画面中心。
fn bg_affine_set(cpu: &mut Cpu, bus: &mut Bus) {
    let mut src = cpu.regs[0];
    let mut dst = cpu.regs[1];
    for _ in 0..cpu.regs[2] {
        let ox = bus.read32(src) as i32 as f64 / 256.0;
        let oy = bus.read32(src.wrapping_add(4)) as i32 as f64 / 256.0;
        let cx = bus.read16(src.wrapping_add(8)) as i16 as f64;
        let cy = bus.read16(src.wrapping_add(10)) as i16 as f64;
        let sx = bus.read16(src.wrapping_add(12)) as i16 as f64 / 256.0;
        let sy = bus.read16(src.wrapping_add(14)) as i16 as f64 / 256.0;
        // 角度は上位 8bit のみ有効 (0..0xFFFF = 0..2π)
        let theta = (bus.read16(src.wrapping_add(16)) & 0xFF00) as f64 / 65536.0 * TAU;
        src = src.wrapping_add(20);
        let (sin, cos) = (libm::sin(theta), libm::cos(theta));
        let pa = sx * cos;
        let pb = -sx * sin;
        let pc = sy * sin;
        let pd = sy * cos;
        bus.write16(dst, (pa * 256.0) as i32 as u16);
        bus.write16(dst.wrapping_add(2), (pb * 256.0) as i32 as u16);
        bus.write16(dst.wrapping_add(4), (pc * 256.0) as i32 as u16);
        bus.write16(dst.wrapping_add(6), (pd * 256.0) as i32 as u16);
        bus.write32(dst.wrapping_add(8), ((ox - pa * cx - pb * cy) * 256.0) as i32 as u32);
        bus.write32(dst.wrapping_add(12), ((oy - pc * cx - pd * cy) * 256.0) as i32 as u32);
        dst = dst.wrapping_add(16);
    }
}

fn obj_affine_set(cpu: &mut Cpu, bus: &mut Bus) {
    let mut src = cpu.regs[0];
    let mut dst = cpu.regs[1];
    let offset = cpu.regs[3];
    for _ in 0..cpu.regs[2] {
        let sx = bus.read16(src) as i16 as f64 / 256.0;
        let sy = bus.read16(src.wrapping_add(2)) as i16 as f64 / 256.0;
        let theta = (bus.read16(src.wrapping_add(4)) & 0xFF00) as f64 / 65536.0 * TAU;
        src = src.wrapping_add(8);
        let (sin, cos) = (libm::sin(theta), libm::cos(theta));
        bus.write16(dst, (sx * cos * 256.0) as i32 as u16);
        bus.write16(dst.wrapping_add(offset), (-sx * sin * 256.0) as i32 as u16);
        bus.write16(dst.wrapping_add(offset * 2), (sy * sin * 256.0) as i32 as u16);
        bus.write16(dst.wrapping_add(offset * 3), (sy * cos * 256.0) as i32 as u16);
        dst = dst.wrapping_add(offset * 4);
    }
}

/// LZ77UnComp。VRAM 宛でも安全なように 16bit 単位で書き出す
/// （伸長サイズは常に偶数であることを前提とする）。
fn lz77(cpu: &mut Cpu, bus: &mut Bus) {
    let src = cpu.regs[0];
    let dst = cpu.regs[1];
    let size = (bus.read32(src) >> 8) as usize;
    let mut out: Vec<u8> = Vec::with_capacity(size);
    let mut sp = src.wrapping_add(4);
    while out.len() < size {
        let flags = bus.read8(sp);
        sp = sp.wrapping_add(1);
        for bit in (0..8).rev() {
            if out.len() >= size {
                break;
            }
            if flags & 1 << bit != 0 {
                let b0 = bus.read8(sp) as usize;
                let b1 = bus.read8(sp.wrapping_add(1)) as usize;
                sp = sp.wrapping_add(2);
                let len = (b0 >> 4) + 3;
                let disp = ((b0 & 0xF) << 8 | b1) + 1;
                for _ in 0..len {
                    if out.len() >= size {
                        break;
                    }
                    let v = if disp <= out.len() { out[out.len() - disp] } else { 0 };
                    out.push(v);
                }
            } else {
                out.push(bus.read8(sp));
                sp = sp.wrapping_add(1);
            }
        }
    }
    write_halfwords(bus, dst, &out);
}

fn rl_uncomp(cpu: &mut Cpu, bus: &mut Bus) {
    let src = cpu.regs[0];
    let dst = cpu.regs[1];
    let size = (bus.read32(src) >> 8) as usize;
    let mut out: Vec<u8> = Vec::with_capacity(size);
    let mut sp = src.wrapping_add(4);
    while out.len() < size {
        let f = bus.read8(sp);
        sp = sp.wrapping_add(1);
        if f & 0x80 != 0 {
            let len = (f as usize & 0x7F) + 3;
            let v = bus.read8(sp);
            sp = sp.wrapping_add(1);
            for _ in 0..len.min(size - out.len()) {
                out.push(v);
            }
        } else {
            let len = (f as usize & 0x7F) + 1;
            for _ in 0..len.min(size - out.len()) {
                out.push(bus.read8(sp));
                sp = sp.wrapping_add(1);
            }
        }
    }
    write_halfwords(bus, dst, &out);
}

fn write_halfwords(bus: &mut Bus, dst: u32, data: &[u8]) {
    for (i, pair) in data.chunks(2).enumerate() {
        let v = pair[0] as u16 | (*pair.get(1).unwrap_or(&0) as u16) << 8;
        bus.write16(dst.wrapping_add(i as u32 * 2), v);
    }
}
