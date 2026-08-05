//! Linear resampler: converts hardware-rate mic audio to the 16 kHz mono
//! stream that the sherpa-onnx Paraformer model expects.
//!
//! AVAudioEngine refuses to resample on an `inputNode` tap — the tap format
//! must equal the hardware output format (typically 44.1 kHz or 48 kHz).
//! We therefore tap at the native rate and downsample here.
//!
//! State (`pos`, `tail`) is carried across calls so consecutive tap buffers
//! interpolate continuously with no clicks at buffer boundaries.

pub(crate) struct LinearResampler {
    /// Input samples advanced per output sample (`src_rate / dst_rate`).
    step: f64,
    /// Fractional read position within the current `tail ++ input` buffer.
    pos: f64,
    /// Trailing samples from the previous chunk kept for cross-buffer interpolation.
    tail: Vec<f32>,
}

impl LinearResampler {
    pub(crate) fn new(src_rate: f64, dst_rate: f64) -> Self {
        let step = if dst_rate > 0.0 {
            (src_rate / dst_rate).max(f64::MIN_POSITIVE)
        } else {
            1.0
        };
        Self {
            step,
            pos: 0.0,
            tail: Vec::new(),
        }
    }

    /// Resample one chunk of mono `f32` samples. Returns the 16 kHz output for
    /// this chunk; any leftover samples needed for the next chunk are retained.
    pub(crate) fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }

        let mut buf = Vec::with_capacity(self.tail.len() + input.len());
        buf.extend_from_slice(&self.tail);
        buf.extend_from_slice(input);
        let len = buf.len();

        let mut out = Vec::with_capacity((len as f64 / self.step) as usize + 1);
        while self.pos + 1.0 < len as f64 {
            let i = self.pos.floor() as usize;
            let frac = (self.pos - i as f64) as f32;
            out.push(buf[i] * (1.0 - frac) + buf[i + 1] * frac);
            self.pos += self.step;
        }

        // Keep buf[floor(pos)..] — exactly the left-neighbor samples the next
        // chunk needs — and rebase pos onto that retained tail.
        let drop = (self.pos.floor() as usize).min(len);
        self.tail = buf[drop..].to_vec();
        self.pos -= drop as f64;
        out
    }
}

#[cfg(test)]
#[path = "../tests/unit/resample_tests.rs"]
mod tests;
