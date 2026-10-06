// SPDX-License-Identifier: GPL-3.0-or-later
//! Phase-aware safety uses the same fixed rollout and spatial storage as normal
//! controls. Crossing a coil is safe only while intangible; the first tangible
//! sweep and subsequent trajectory must pass ordinary body/head/self checks.
use crate::{SnakeView, World};

#[inline]
pub(super) fn step(time:f64)->usize {crate::effects::forecast_step(time)}
impl super::AiController {
    pub(super) fn phase_mask(&self,w:&World,id:usize,time:f64)->u16 {self.effects.mask(w,id,step(time))}
}
pub(super) fn horizon(w:&World,s:SnakeView<'_>,ordinary:usize)->usize {
    let effect=super::forecast::Effect::observed(w,s);
    if effect.is(crate::effects::EffectKind::Phase) {
        ordinary.max((effect.ticks as usize+18).min(super::STEPS))
    } else if s.inventory.windup!=0 && s.inventory.kinds[s.inventory.windup as usize-1]==crate::effects::EffectKind::Phase as u8 {
        ordinary.max((s.inventory.windup_ticks as usize+1+crate::effects::EffectKind::Phase.duration() as usize+18).min(super::STEPS))
    } else {ordinary}
}
