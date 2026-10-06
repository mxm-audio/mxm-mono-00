//! Per-sample cost of the voice, for the routing conversion's cost gate.
use mxm_mono_00_dsp::routing::Routing;
use mxm_mono_00_dsp::voice::{NoteId, Patch, Voice};
use std::time::Instant;

fn main() {
    let p = Patch::default();
    let mut v = Voice::new();
    v.set_sample_rate(48_000.0);
    let r = Routing::init();
    v.set_topology(&r);
    v.note_on(
        NoteId {
            voice_id: Some(1),
            channel: 0,
            note: 60,
        },
        1.0,
        &p,
    );
    // Warm up.
    for _ in 0..48_000 {
        std::hint::black_box(v.process(&p, &r));
    }
    let n = 48_000 * 20;
    for copied in [false, true] {
        let mut best = f64::INFINITY;
        for _ in 0..4 {
            let t = Instant::now();
            for _ in 0..n {
                if copied {
                    // The plugin rebuilds the patch every sample; this is the same shape.
                    let q = std::hint::black_box(p);
                    std::hint::black_box(v.process(&q, &r));
                } else {
                    std::hint::black_box(v.process(&p, &r));
                }
            }
            let ns = t.elapsed().as_secs_f64() * 1e9 / n as f64;
            best = best.min(ns);
        }
        println!(
            "{best:.1} ns/sample  copied={copied}  (Patch is {} bytes)",
            size_of::<Patch>()
        );
    }
}
