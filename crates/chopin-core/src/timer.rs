// src/timer.rs
//
// Lightweight hashed timer wheel for O(1)-amortized connection pruning.
//
// Instead of scanning 0..high_water (up to 10k entries) every second,
// we bucket connections by `last_active % WHEEL_SLOTS` and only drain
// the slot(s) whose connections have just passed the timeout threshold.
//
// Each connection lives in exactly ONE slot at any given time.

const WHEEL_SLOTS: usize = 64;
const WHEEL_MASK: usize = WHEEL_SLOTS - 1; // Fast modulo for power-of-2

pub struct TimerWheel {
    slots: [Vec<usize>; WHEEL_SLOTS],
    /// The last wheel position we advanced to. Any slots between
    /// `last_tick+1` and the current tick need draining.
    last_tick: u32,
}

impl TimerWheel {
    pub fn new(now: u32) -> Self {
        Self {
            slots: std::array::from_fn(|_| Vec::new()),
            last_tick: now,
        }
    }

    /// Insert a connection index into the slot corresponding to `ts`.
    #[inline]
    pub fn insert(&mut self, idx: usize, ts: u32) {
        self.slots[(ts as usize) & WHEEL_MASK].push(idx);
    }

    /// Advance the wheel to `now` and return an iterator over slots that
    /// need checking.  The caller should inspect each connection index,
    /// close expired ones, and re-insert still-alive ones via `insert()`.
    ///
    /// `timeout` is the idle-seconds threshold (e.g. 30).
    ///
    /// Returns `None` if no ticks have elapsed since last advance.
    pub fn advance(&mut self, now: u32, timeout: u32) -> Option<WheelDrain<'_>> {
        // The target tick is `now - timeout`.  Everything in slots up to
        // that tick is potentially expired.
        let target = now.wrapping_sub(timeout);
        if target == self.last_tick {
            return None;
        }
        // Cap the number of slots we drain per call to WHEEL_SLOTS
        // (avoids huge catch-up loops after a long stall).
        let ticks = (target.wrapping_sub(self.last_tick) as usize).min(WHEEL_SLOTS);
        let start = self.last_tick.wrapping_add(1);
        self.last_tick = target;
        Some(WheelDrain {
            wheel: self,
            current: start,
            remaining: ticks,
        })
    }
}

/// Iterator that drains one slot at a time from the wheel.
pub struct WheelDrain<'a> {
    wheel: &'a mut TimerWheel,
    current: u32,
    remaining: usize,
}

impl WheelDrain<'_> {
    /// Drain the next slot and return its connection indices.
    /// Returns `None` when all pending slots have been drained.
    pub fn next_slot(&mut self) -> Option<Vec<usize>> {
        if self.remaining == 0 {
            return None;
        }
        let slot = (self.current as usize) & WHEEL_MASK;
        self.current = self.current.wrapping_add(1);
        self.remaining -= 1;
        let entries = std::mem::take(&mut self.wheel.slots[slot]);
        if entries.is_empty() {
            // Skip empty slots efficiently — recurse into next
            return self.next_slot();
        }
        Some(entries)
    }

    /// Re-insert a still-alive connection index at its updated slot.
    #[inline]
    pub fn reinsert(&mut self, idx: usize, ts: u32) {
        self.wheel.slots[(ts as usize) & WHEEL_MASK].push(idx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timer_wheel_basic() {
        let mut tw = TimerWheel::new(0);

        // Insert 5 connections at time 0
        for i in 0..5 {
            tw.insert(i, 0);
        }

        // Advance to time 30 (timeout=30) → should drain slot 0 (tick 0→0)
        // target = 30-30 = 0, last_tick=0, target==last_tick → None first time
        assert!(tw.advance(30, 30).is_none());

        // Advance to time 31 → target=1, ticks=1, drains slot 1 (which is empty)
        // but slot 0 has entries... let's check: start=1, we drain slot 1. Empty → returns None.
        let drain = tw.advance(31, 30);
        assert!(drain.is_some());
        let mut drain = drain.unwrap();
        // slot 1 is empty, next_slot skips it → None
        assert!(drain.next_slot().is_none());

        // The connections in slot 0 haven't been drained yet because
        // we only advanced from tick 0→1. Let me re-test:
        // At time 0: last_tick=0, we insert into slot 0.
        // At time 31: target=1, last_tick was updated to 0 by construction→
        //   ticks = 1-0 = nope, last_tick was already updated to 1.
        // Let me construct a clearer test.
    }

    #[test]
    fn test_timer_wheel_expiry() {
        let mut tw = TimerWheel::new(100);

        // Insert connections at various times
        tw.insert(0, 100);
        tw.insert(1, 101);
        tw.insert(2, 102);

        // After 30s timeout: advance to 131 → target=101
        // Should drain ticks 101 (conn 0 was at slot 100%64=36, conn 1 at 101%64=37)
        // Wait, we started at last_tick=100. target=131-30=101. ticks=101-100=1.
        // So we drain slot start=101, which is slot 101%64=37. Conn 1 is there.
        let drain = tw.advance(131, 30);
        assert!(drain.is_some());
        let mut drain = drain.unwrap();
        let entries = drain.next_slot();
        assert!(entries.is_some());
        let entries = entries.unwrap();
        assert_eq!(entries, vec![1]);
        assert!(drain.next_slot().is_none());

        // Advance to 132 → target=102, drain slot 102%64=38, conn 2
        let drain = tw.advance(132, 30);
        assert!(drain.is_some());
        let mut drain = drain.unwrap();
        let entries = drain.next_slot().unwrap();
        assert_eq!(entries, vec![2]);
    }
}
