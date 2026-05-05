const WINDOW_SIZE: u64 = 2048;
const BITMAP_WORDS: usize = (WINDOW_SIZE as usize) / 64;

pub struct ReplayWindow {
    top: u64,
    bitmap: [u64; BITMAP_WORDS],
}

impl ReplayWindow {
    pub fn new() -> Self {
        Self {
            top: 0,
            bitmap: [0u64; BITMAP_WORDS],
        }
    }

    /// Check if a counter value is acceptable (not replayed, not too old).
    /// Returns true if the packet should be processed. On true, marks counter as seen.
    pub fn check_and_mark(&mut self, counter: u64) -> bool {
        if counter == 0 && self.top == 0 && self.bitmap[0] == 0 {
            self.top = counter;
            self.bitmap[0] = 1;
            return true;
        }

        if counter > self.top {
            let shift = counter - self.top;
            self.advance(shift);
            self.top = counter;
            self.bitmap[0] |= 1;
            return true;
        }

        let diff = self.top - counter;
        if diff >= WINDOW_SIZE {
            return false;
        }

        let word_idx = (diff as usize) / 64;
        let bit_idx = (diff as usize) % 64;

        if self.bitmap[word_idx] & (1u64 << bit_idx) != 0 {
            return false;
        }

        self.bitmap[word_idx] |= 1u64 << bit_idx;
        true
    }

    fn advance(&mut self, shift: u64) {
        if shift >= WINDOW_SIZE {
            self.bitmap = [0u64; BITMAP_WORDS];
            return;
        }

        let word_shift = (shift as usize) / 64;
        let bit_shift = (shift as usize) % 64;

        if word_shift > 0 {
            for i in (word_shift..BITMAP_WORDS).rev() {
                self.bitmap[i] = self.bitmap[i - word_shift];
            }
            for i in 0..word_shift {
                self.bitmap[i] = 0;
            }
        }

        if bit_shift > 0 {
            for i in (1..BITMAP_WORDS).rev() {
                self.bitmap[i] =
                    (self.bitmap[i] << bit_shift) | (self.bitmap[i - 1] >> (64 - bit_shift));
            }
            self.bitmap[0] <<= bit_shift;
        }
    }
}

/// Simple monotonic counter check for TCP (must strictly increase).
pub struct TcpCounterCheck {
    last: Option<u64>,
}

impl TcpCounterCheck {
    pub fn new() -> Self {
        Self { last: None }
    }

    pub fn check(&mut self, counter: u64) -> bool {
        match self.last {
            None => {
                self.last = Some(counter);
                true
            }
            Some(prev) => {
                if counter > prev {
                    self.last = Some(counter);
                    true
                } else {
                    false
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_packets_accepted() {
        let mut w = ReplayWindow::new();
        for i in 0..100 {
            assert!(w.check_and_mark(i), "counter {i} should be accepted");
        }
    }

    #[test]
    fn duplicate_rejected() {
        let mut w = ReplayWindow::new();
        assert!(w.check_and_mark(5));
        assert!(!w.check_and_mark(5));
    }

    #[test]
    fn out_of_order_within_window() {
        let mut w = ReplayWindow::new();
        assert!(w.check_and_mark(0));
        assert!(w.check_and_mark(10));
        assert!(w.check_and_mark(5));
        assert!(!w.check_and_mark(5));
    }

    #[test]
    fn too_old_rejected() {
        let mut w = ReplayWindow::new();
        assert!(w.check_and_mark(0));
        assert!(w.check_and_mark(WINDOW_SIZE + 100));
        assert!(!w.check_and_mark(0));
        assert!(!w.check_and_mark(50));
    }

    #[test]
    fn large_jump_resets_window() {
        let mut w = ReplayWindow::new();
        assert!(w.check_and_mark(0));
        assert!(w.check_and_mark(WINDOW_SIZE * 2));
        assert!(!w.check_and_mark(0));
        assert!(!w.check_and_mark(WINDOW_SIZE));
    }

    #[test]
    fn tcp_counter_strictly_increasing() {
        let mut c = TcpCounterCheck::new();
        assert!(c.check(0));
        assert!(c.check(1));
        assert!(c.check(5));
        assert!(!c.check(5));
        assert!(!c.check(3));
        assert!(c.check(6));
    }
}
