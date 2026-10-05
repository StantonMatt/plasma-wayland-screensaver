// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
#[inline]
pub(super) fn ice(c:Color,flags:u32,palette:&[Color])->Color {
    if flags&flags::FROZEN==0 {return c;}
    let tint=items::tint(Color::new(200,238,255,255),palette);
    let mix=|a:u8,b:u8|((a as u16*2+b as u16*3+2)/5) as u8;
    Color::new(mix(c.red,tint.red),mix(c.green,tint.green),mix(c.blue,tint.blue),c.alpha)
}
