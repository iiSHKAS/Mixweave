use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

/// Maximum concurrent meters: up to 10 channels + up to 6 buses (master,
/// Streamer Mode, up to 4 custom mixes) + up to 4 mic channels (primary +
/// secondary), each channel's and each mic's independent Stream meter
/// alongside it, and headroom.
pub const MAX_METERS: usize = 40;

/// Lock-free per-meter peak store with a dynamic name→slot registry
/// (channels are user-defined since the dynamic-channels work). Peaks are
/// written by realtime meter/DSP callbacks and drained by the level
/// emitter; values are f32 amplitudes bit-cast into AtomicU32.
pub struct LevelStore {
    peaks: [[AtomicU32; 2]; MAX_METERS],
    slots: Mutex<SlotRegistry>,
}

#[derive(Default)]
struct SlotRegistry {
    by_name: HashMap<String, usize>,
    free: Vec<usize>,
}

impl LevelStore {
    pub fn new() -> Self {
        Self {
            // `[T; N]: Default` is only implemented up to N=32 (no true
            // const-generic impl in std) - MAX_METERS has since grown past
            // that, so this builds the array element-by-element instead.
            peaks: std::array::from_fn(|_| [AtomicU32::new(0), AtomicU32::new(0)]),
            slots: Mutex::new(SlotRegistry::default()),
        }
    }

    /// Slot for `name`, registering it on first use. None when the meter
    /// budget is exhausted.
    pub fn slot_for(&self, name: &str) -> Option<usize> {
        let mut registry = self.slots.lock().ok()?;
        if let Some(slot) = registry.by_name.get(name) {
            return Some(*slot);
        }
        let slot = registry.free.pop().or_else(|| {
            let next = registry.by_name.len() + registry.free.len();
            (next < MAX_METERS).then_some(next)
        })?;
        registry.by_name.insert(name.to_string(), slot);
        Some(slot)
    }

    /// Free a name's slot for reuse (channel deleted).
    pub fn release(&self, name: &str) {
        if let Ok(mut registry) = self.slots.lock() {
            if let Some(slot) = registry.by_name.remove(name) {
                self.peaks[slot][0].store(0, Ordering::Relaxed);
                self.peaks[slot][1].store(0, Ordering::Relaxed);
                registry.free.push(slot);
            }
        }
    }

    /// Snapshot of registered meter names and their slots.
    pub fn names(&self) -> Vec<(String, usize)> {
        self.slots
            .lock()
            .map(|r| r.by_name.iter().map(|(n, s)| (n.clone(), *s)).collect())
            .unwrap_or_default()
    }

    /// Raise the stored peak for a meter channel (kept until drained).
    pub fn raise(&self, slot: usize, channel: usize, amplitude: f32) {
        let Some(cell) = self.peaks.get(slot).map(|p| &p[channel.min(1)]) else {
            return;
        };
        let new = amplitude.to_bits();
        let mut current = cell.load(Ordering::Relaxed);
        while f32::from_bits(current) < amplitude {
            match cell.compare_exchange_weak(current, new, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    /// Read and reset the peak for a meter channel.
    pub fn drain(&self, slot: usize, channel: usize) -> f32 {
        self.peaks
            .get(slot)
            .map(|p| f32::from_bits(p[channel.min(1)].swap(0, Ordering::Relaxed)))
            .unwrap_or(0.0)
    }

    /// Reset every peak without locking the dynamic name registry. Used while
    /// visual metering is suppressed so an old maximum cannot flash when the
    /// window or meters become active again.
    pub fn discard_all(&self) {
        for peak in &self.peaks {
            peak[0].store(0, Ordering::Relaxed);
            peak[1].store(0, Ordering::Relaxed);
        }
    }
}

impl Default for LevelStore {
    fn default() -> Self {
        Self::new()
    }
}
