//! Absolute rational tick arithmetic; callback order never changes simulation steps.
use std::collections::VecDeque;
use std::sync::Arc;
#[derive(Clone, Copy, Debug)]
pub struct Rate {
    pub(crate) numerator: u32,
    pub(crate) denominator: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct Bracket {
    pub lower: u64,
    pub upper: u64,
    pub alpha: f64,
}
impl Rate {
    pub fn new(numerator: u32, denominator: u32) -> Option<Self> {
        (numerator > 0 && numerator <= 1_000_000_000 && denominator > 0).then_some(Self {
            numerator,
            denominator,
        })
    }
    pub fn bracket(self, ns: i64) -> Bracket {
        let n = i128::from(ns.max(0)) * i128::from(self.numerator);
        let d = 1_000_000_000_i128 * i128::from(self.denominator);
        let lower = (n / d) as u64;
        Bracket {
            lower,
            upper: lower + 1,
            alpha: (n % d) as f64 / d as f64,
        }
    }
    pub fn tick_ns(self, tick: u64) -> i64 {
        (i128::from(tick) * 1_000_000_000 * i128::from(self.denominator)
            / i128::from(self.numerator))
        .min(i128::from(i64::MAX)) as i64
    }
    pub fn history_capacity(self, lead_ns: i64) -> usize {
        let n = i128::from(lead_ns.clamp(0, 1_000_000_000)) * i128::from(self.numerator);
        let d = 1_000_000_000_i128 * i128::from(self.denominator);
        ((n + d - 1) / d) as usize + 2
    }
}
pub struct History<T> {
    entries: VecDeque<(u64, Arc<T>)>,
    capacity: usize,
}
pub struct Sample<T> {
    pub lower: Arc<T>,
    pub upper: Arc<T>,
    pub alpha: f64,
    pub missed: bool,
}
impl<T> History<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity.max(2)),
            capacity: capacity.max(2),
        }
    }
    pub fn push(&mut self, tick: u64, value: T) {
        assert!(self.entries.back().is_none_or(|(last, _)| tick > *last));
        // A retained lease owns its endpoints. Recycle only an evicted slot
        // that has no external readers, as in the steady presentation loop.
        let retired =
            (self.entries.len() == self.capacity).then(|| self.entries.pop_front().unwrap().1);
        let slot = match retired {
            Some(mut slot) if Arc::strong_count(&slot) == 1 && Arc::weak_count(&slot) == 0 => {
                *Arc::get_mut(&mut slot).unwrap() = value;
                slot
            }
            _ => Arc::new(value),
        };
        self.entries.push_back((tick, slot));
    }
    pub fn sample(&self, bracket: Bracket) -> Option<Sample<T>> {
        let (first, oldest) = self.entries.front()?;
        let (last, newest) = self.entries.back()?;
        if bracket.lower < *first {
            return Some(Sample {
                lower: oldest.clone(),
                upper: oldest.clone(),
                alpha: 0.,
                missed: true,
            });
        }
        if bracket.upper > *last {
            return Some(Sample {
                lower: newest.clone(),
                upper: newest.clone(),
                alpha: 0.,
                missed: true,
            });
        }
        let i = (bracket.lower - first) as usize;
        Some(Sample {
            lower: self.entries[i].1.clone(),
            upper: self.entries[i + 1].1.clone(),
            alpha: bracket.alpha,
            missed: false,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interleaved_presentations() {
        let rate = Rate::new(60, 1).unwrap();
        let requests: Vec<i64> = [60, 100, 175, 240]
            .iter()
            .flat_map(|hz| (0..*hz).map(move |n| i64::from(n) * 1_000_000_000 / i64::from(*hz)))
            .collect();
        for order in 0..3 {
            let mut callbacks = requests.clone();
            if order < 2 {
                callbacks.sort();
            }
            if order == 1 {
                callbacks.reverse();
            }
            // Third order batches each output's callbacks, so late outputs
            // sample old endpoints without causing any duplicate steps.
            let mut history = History::new(rate.history_capacity(1_000_000_000));
            history.push(0, 0);
            let mut stepped = 0;
            for t in &callbacks {
                let b = rate.bracket(*t);
                while stepped < b.upper {
                    stepped += 1;
                    history.push(stepped, stepped);
                }
                let a = history.sample(b).unwrap();
                let c = history.sample(b).unwrap();
                assert_eq!((*a.lower, *a.upper, a.alpha), (*c.lower, *c.upper, c.alpha));
                assert_eq!(*a.lower, b.lower);
            }
            assert_eq!(stepped, 60);
        }
    }
    #[test]
    fn rational_and_clamp_and_leases() {
        let rate = Rate::new(60000, 1001).unwrap();
        assert_eq!(rate.bracket(1_001_000_000).lower, 60);
        let mut h = History::new(2);
        h.push(0, 0);
        h.push(1, 1);
        let lease = h.sample(rate.bracket(0)).unwrap();
        h.push(2, 2);
        let miss = h.sample(rate.bracket(0)).unwrap();
        assert!(miss.missed);
        assert_eq!(*miss.lower, 1);
        drop(h);
        assert_eq!(*lease.lower, 0);
        assert_eq!(Rate::new(240, 1).unwrap().history_capacity(20_000_000), 7);
        assert_eq!(rate.bracket(i64::MAX).lower, 552849472738);
    }
}
