//! Local-calendar seasonal policy. No persistence, wall-clock overrides, or timers.

use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Costume {
    Fangs,
    Drips,
    Bat,
}

pub struct Halloween {
    date: Option<(i32, u32, u32)>,
    active: Option<Costume>,
    previous: Option<Costume>,
    next: Duration,
    random: u64,
}

impl Halloween {
    pub fn new(seed: u64) -> Self {
        Self {
            date: None,
            active: None,
            previous: None,
            next: Duration::ZERO,
            random: seed.max(1),
        }
    }

    fn random(&mut self) -> u64 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        self.random
    }

    /// Called from the existing one-second UI clock. Monotonic elapsed time
    /// spaces changes out; local calendar time only gates October 31.
    pub fn update(
        &mut self,
        date: (i32, u32, u32),
        enabled: bool,
        elapsed: Duration,
    ) -> Option<Costume> {
        if !enabled || date.1 != 10 || date.2 != 31 {
            self.date = None;
            self.active = None;
            self.next = Duration::ZERO;
            return None;
        }
        if self.date != Some(date) {
            self.date = Some(date);
            self.active = None;
            self.next = elapsed;
        }
        if elapsed >= self.next {
            if self.active.take().is_some() {
                // Quiet stretches of two to four minutes between 20–35 s visits.
                self.next = elapsed + Duration::from_secs(120 + self.random() % 121);
            } else {
                let choices = [Costume::Fangs, Costume::Drips, Costume::Bat];
                let mut index = (self.random() % 3) as usize;
                if Some(choices[index]) == self.previous {
                    index = (index + 1 + (self.random() % 2) as usize) % 3;
                }
                self.active = Some(choices[index]);
                self.previous = self.active;
                self.next = elapsed + Duration::from_secs(20 + self.random() % 16);
            }
        }
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_date_enable_and_midnight_cleanup_are_independent_of_first_run() {
        let mut h = Halloween::new(10);
        for date in [(2026, 10, 10), (2026, 10, 30), (2026, 11, 1), (2026, 1, 31)] {
            assert_eq!(h.update(date, true, Duration::ZERO), None);
        }
        assert!(h
            .update((2026, 10, 31), true, Duration::from_secs(1))
            .is_some());
        assert_eq!(h.update((2026, 11, 1), true, Duration::from_secs(2)), None);
        assert_eq!(
            h.update((2027, 10, 31), false, Duration::from_secs(3)),
            None
        );
        assert!(h
            .update((2027, 10, 31), true, Duration::from_secs(4))
            .is_some());
    }

    #[test]
    fn all_three_motifs_have_bounded_visits_and_quiet_nonrepeating_changes() {
        let mut h = Halloween::new(31);
        let date = (2026, 10, 31);
        let mut seen = Vec::new();
        let mut elapsed = Duration::ZERO;
        for _ in 0..30 {
            let motif = h.update(date, true, elapsed).unwrap();
            assert_ne!(seen.last(), Some(&motif));
            seen.push(motif);
            let active_until = h.next;
            assert!((20..=35).contains(&(active_until - elapsed).as_secs()));
            assert_eq!(
                h.update(date, true, active_until - Duration::from_secs(1)),
                Some(motif)
            );
            assert_eq!(h.update(date, true, active_until), None);
            assert!((120..=240).contains(&(h.next - active_until).as_secs()));
            assert_eq!(h.update(date, true, h.next - Duration::from_secs(1)), None);
            elapsed = h.next;
        }
        for motif in [Costume::Fangs, Costume::Drips, Costume::Bat] {
            assert!(seen.contains(&motif));
        }
    }

    #[test]
    fn suspend_and_clock_changes_do_not_accumulate_animation_work() {
        let mut h = Halloween::new(1);
        assert!(h.update((2026, 10, 31), true, Duration::ZERO).is_some());
        assert_eq!(
            h.update((2026, 10, 31), true, Duration::from_secs(36000)),
            None
        );
        assert!(h.next > Duration::from_secs(36000));
        assert_eq!(
            h.update((2026, 10, 30), true, Duration::from_secs(36001)),
            None
        );
    }
}
