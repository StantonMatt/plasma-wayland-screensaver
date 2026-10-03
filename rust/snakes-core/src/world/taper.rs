// SPDX-License-Identifier: GPL-3.0-or-later
//! Contact margins remain unchanged; only the body's half-width tapers.
use super::RuleSet;

#[inline]
pub(crate) fn body_radius(radius: f64, index: f64, len: usize) -> f64 {
    radius * crate::shape::taper(index / len.saturating_sub(1).max(1) as f64)
}

/// Conservative radius over a continuous body edge, including the wide neck.
#[inline]
pub(crate) fn span_radius(radius: f64, lo: f64, hi: f64) -> f64 {
    if lo <= 0.6 && hi >= 0.07 { radius }
    else { radius * crate::shape::taper(lo).max(crate::shape::taper(hi)) }
}

#[inline]
pub(crate) fn contact_radius(rules: RuleSet, head: f64, body: f64, same: bool) -> f64 {
    if rules == RuleSet::Classic && same { head * 1.48 }
    else { (head + body) * if same { 0.74 } else { 0.78 } }
}
