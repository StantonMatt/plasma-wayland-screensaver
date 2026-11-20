// SPDX-License-Identifier: GPL-3.0-or-later
//! IDs: ocean, spectrum, ember, forest, mono, pastel (unknown QML names use ocean).
pub const COLORS: [[u32; 6]; 6] = [
    [0xd9fbff, 0x3dd6e8, 0x3a86ff, 0x7358d6, 0x2aa889, 0x9bf6ff],
    [0xff477e, 0xffbe0b, 0x42e2b8, 0x3a86ff, 0xb967ff, 0xfb5607],
    [0xfff1a8, 0xffc857, 0xff7b42, 0xef3e36, 0x9c1c28, 0xffd6a5],
    [0xd8f3dc, 0x95d5b2, 0x52b788, 0x2d6a4f, 0xb7e4c7, 0x74c69d],
    [0xffffff, 0xd9e1e8, 0xaeb8c2, 0x7f8b96, 0xedf2f4, 0xbac4ce],
    [0xffc8dd, 0xbde0fe, 0xcaffbf, 0xffd6a5, 0xe7c6ff, 0xa2d2ff],
];
pub fn color(palette: u32, index: usize) -> [f32; 4] {
    let c = COLORS[palette as usize][index % 6];
    [
        ((c >> 16) & 255) as f32 / 255.0,
        ((c >> 8) & 255) as f32 / 255.0,
        (c & 255) as f32 / 255.0,
        1.0,
    ]
}
