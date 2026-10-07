//! Whether this PC is slow for Overseer, from what it has. On a slow one the
//! game needs every bit of graphics it can get, so clips are cut at 720p.

/// Less memory than this, in bytes, counts as slow. A PC with 8 GB reads as
/// a little under 8, so the line sits below it.
const MEMORY_LEAST: u64 = 7 << 30;

/// This many threads or fewer counts as slow, as every processor in
/// VALORANT's minimum and recommended specs has.
const THREADS_MOST: usize = 4;

/// What this PC has.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Machine {
    /// Its memory in bytes, when Windows says.
    pub(crate) memory: Option<u64>,
    /// How many threads its processor runs at once.
    pub(crate) threads: usize,
    /// Whether it has a graphics card of its own, rather than only the chip
    /// in the processor that VALORANT would share with this app.
    pub(crate) card: bool,
}

impl Machine {
    /// What this PC has, read once at start up. Nothing here starts a driver.
    pub(crate) fn detect() -> Self {
        Self {
            memory: overseer_native::memory(),
            threads: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
            card: overseer_native::graphics_card(),
        }
    }

    /// Why it counts as slow, a few words for each reason, or nothing when it
    /// doesn't.
    pub(crate) fn slow_because(&self) -> Vec<String> {
        let mut why = Vec::new();
        if let Some(memory) = self.memory.filter(|m| *m < MEMORY_LEAST) {
            why.push(format!("{} GB of memory", (memory + (1 << 29)) >> 30));
        }
        if self.threads <= THREADS_MOST {
            why.push(format!("{} processor threads", self.threads));
        }
        if !self.card {
            why.push("no graphics card".to_owned());
        }
        why
    }
}

#[cfg(test)]
mod tests {
    use super::Machine;

    /// VALORANT's minimum spec counts as slow on every count, a gaming
    /// laptop on none, and a laptop with only the processor's graphics on
    /// that alone.
    #[test]
    fn a_pc_is_slow_for_what_it_lacks() {
        let least = Machine {
            memory: Some(4 << 30),
            threads: 4,
            card: false,
        };
        assert_eq!(
            least.slow_because(),
            ["4 GB of memory", "4 processor threads", "no graphics card"]
        );
        let gaming = Machine {
            memory: Some(31 << 30),
            threads: 16,
            card: true,
        };
        assert!(gaming.slow_because().is_empty());
        let thin = Machine {
            card: false,
            ..gaming
        };
        assert_eq!(thin.slow_because(), ["no graphics card"]);
        let eight = Machine {
            memory: Some((8 << 30) - (200 << 20)),
            ..gaming
        };
        assert!(
            eight.slow_because().is_empty(),
            "8 GB as Windows reports it"
        );
    }
}
