//! Minimal lock-free SPSC ring buffer for f32 samples, connecting the mic
//! capture callback (producer) to the virtual-source playback callback
//! (consumer). Both run on PipeWire data threads; no locks, no allocation.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

pub struct Ring {
    buf: Box<[AtomicU32]>,
    /// Next write position (producer-owned).
    write: AtomicUsize,
    /// Next read position (consumer-owned).
    read: AtomicUsize,
}

impl Ring {
    /// Capacity is rounded up to a power of two. Monotonic cursors distinguish
    /// full from empty, so the complete allocation is usable.
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two().max(2);
        let buf = (0..cap).map(|_| AtomicU32::new(0)).collect::<Vec<_>>();
        Self {
            buf: buf.into_boxed_slice(),
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        }
    }

    fn mask(&self) -> usize {
        self.buf.len() - 1
    }

    /// Push as many samples as fit and return the count accepted. New samples
    /// are dropped on overflow rather than overwriting unread slots: writing
    /// over a slot before publishing the new cursor lets a concurrent reader
    /// observe a torn mixture of old and future audio.
    pub fn push(&self, samples: &[f32]) -> usize {
        let mask = self.mask();
        let mut w = self.write.load(Ordering::Relaxed);
        let r = self.read.load(Ordering::Acquire);
        let used = w.wrapping_sub(r).min(self.buf.len());
        let accepted = samples.len().min(self.buf.len() - used);
        for &s in &samples[..accepted] {
            self.buf[w & mask].store(s.to_bits(), Ordering::Relaxed);
            w = w.wrapping_add(1);
        }
        self.write.store(w, Ordering::Release);
        accepted
    }

    /// Pop up to `out.len()` samples; unfilled tail is zeroed (underrun).
    /// Returns the number of real samples written.
    pub fn pop(&self, out: &mut [f32]) -> usize {
        let mask = self.mask();
        let w = self.write.load(Ordering::Acquire);
        let mut r = self.read.load(Ordering::Relaxed);
        let avail = w.wrapping_sub(r).min(out.len());
        for slot in out.iter_mut().take(avail) {
            *slot = f32::from_bits(self.buf[r & mask].load(Ordering::Relaxed));
            r = r.wrapping_add(1);
        }
        self.read.store(r, Ordering::Release);
        for slot in out.iter_mut().skip(avail) {
            *slot = 0.0;
        }
        avail
    }

    /// Samples currently waiting to be popped (consumer-side view).
    pub fn available(&self) -> usize {
        let w = self.write.load(Ordering::Acquire);
        let r = self.read.load(Ordering::Relaxed);
        w.wrapping_sub(r).min(self.buf.len())
    }

    /// Snapshot the producer's next write position. The producer uses this to
    /// publish an exact lifecycle boundary before it writes resumed audio.
    pub fn write_position(&self) -> usize {
        self.write.load(Ordering::Relaxed)
    }

    /// Consumer-side lifecycle boundary: discard samples only through a
    /// producer cursor captured before fresh audio was published. If the
    /// consumer has already passed the boundary, leave its cursor unchanged.
    pub fn discard_through(&self, boundary: usize) -> usize {
        let r = self.read.load(Ordering::Relaxed);
        let discarded = boundary.wrapping_sub(r);
        if discarded > self.buf.len() {
            return 0;
        }
        self.read.store(boundary, Ordering::Release);
        discarded
    }

}
