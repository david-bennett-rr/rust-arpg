use std::time::{SystemTime, UNIX_EPOCH};

const ZERO_SEED_FALLBACK: u64 = 0xA2F1_9C37_5D4B_E821;

/// Small, deterministic SplitMix64 RNG used for procedural content and damage rolls.
#[derive(Clone, Copy, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { ZERO_SEED_FALLBACK } else { seed },
        }
    }

    pub fn from_entropy() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(ZERO_SEED_FALLBACK);
        Self::new(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn next_usize(&mut self, max: usize) -> usize {
        if max == 0 {
            return 0;
        }

        (self.next_u64() % max as u64) as usize
    }
}

impl Default for SplitMix64 {
    fn default() -> Self {
        Self::from_entropy()
    }
}

#[cfg(test)]
mod tests {
    use super::SplitMix64;

    #[test]
    fn same_seed_produces_same_sequence() {
        let mut a = SplitMix64::new(123);
        let mut b = SplitMix64::new(123);

        for _ in 0..8 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
}
