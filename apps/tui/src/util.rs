//! Small utilities: a deterministic-enough RNG and number formatting.

use std::cell::Cell;

thread_local! {
    static SEED: Cell<u64> = Cell::new(seed_from_clock());
}

fn seed_from_clock() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
        .unwrap_or(0x2545_F491_4F6C_DD1D);
    nanos | 1
}

/// xorshift64*: plenty random for pacing jitter and phrasing variety.
fn next_u64() -> u64 {
    SEED.with(|cell| {
        let mut x = cell.get();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        cell.set(x);
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    })
}

/// Uniform integer in `[low, high]`.
pub fn rand_range(low: u64, high: u64) -> u64 {
    if high <= low {
        return low;
    }
    low + next_u64() % (high - low + 1)
}

/// Pick one element; panics only on an empty slice, which never ships.
pub fn pick<T>(items: &[T]) -> &T {
    &items[(next_u64() as usize) % items.len()]
}
