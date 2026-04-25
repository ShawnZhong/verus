// verus-explorer polyfill. `std::time::Instant::now()` panics on
// wasm32-unknown-unknown (no monotonic clock). Re-export std's type on native;
// on wasm, back it with `performance.now()` so elapsed-time stats and
// per-Z3-round-trip transcript timings both produce real numbers.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;

#[cfg(target_arch = "wasm32")]
pub use wasm::Instant;

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::ops::Sub;
    use std::time::Duration;

    // Renamed from `now` to `air_perf_now` so the wasm-bindgen descriptor
    // symbol doesn't collide with the explorer's identically-wrapped
    // `perf_now` (descriptors are keyed by Rust-side fn name + signature;
    // two extern fns wrapping `js_namespace = performance, js_name = now`
    // need distinct Rust names).
    #[wasm_bindgen::prelude::wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = performance, js_name = now)]
        fn air_perf_now() -> f64;
    }

    #[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
    pub struct Instant(f64); // milliseconds, performance.now() epoch

    impl Instant {
        pub fn now() -> Self {
            Instant(air_perf_now())
        }
        pub fn elapsed(&self) -> Duration {
            Self::now() - *self
        }
        pub fn duration_since(&self, earlier: Self) -> Duration {
            *self - earlier
        }
    }

    impl Sub for Instant {
        type Output = Duration;
        fn sub(self, rhs: Self) -> Duration {
            // performance.now() is monotonic non-decreasing within a context,
            // so saturating at zero handles the rare reordered-call case.
            let ms = (self.0 - rhs.0).max(0.0);
            Duration::from_secs_f64(ms / 1_000.0)
        }
    }
}
