use std::collections::VecDeque;

pub struct AudioJitterBuffer {
    frames: VecDeque<Vec<f32>>,
    capacity: usize,
}

impl AudioJitterBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            frames: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push_frame(&mut self, samples: &[f32]) {
        if self.frames.len() == self.capacity {
            self.frames.pop_front();
        }
        self.frames.push_back(samples.to_vec());
    }

    pub fn pull_frame(&mut self, out: &mut [f32]) -> bool {
        match self.frames.pop_front() {
            Some(frame) => {
                let copy_len = frame.len().min(out.len());
                out[..copy_len].copy_from_slice(&frame[..copy_len]);
                if copy_len < out.len() {
                    out[copy_len..].fill(0.0);
                }
                true
            }
            None => false,
        }
    }

    pub fn level(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn clear(&mut self) {
        self.frames.clear();
    }
}
