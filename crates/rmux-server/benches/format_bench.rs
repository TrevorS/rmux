//! Format string expansion benchmarks.

use criterion::{Criterion, criterion_group, criterion_main};

fn bench_placeholder(_c: &mut Criterion) {
    // Format expansion benchmarks will be added when the format module is implemented
}

criterion_group!(benches, bench_placeholder);
criterion_main!(benches);
