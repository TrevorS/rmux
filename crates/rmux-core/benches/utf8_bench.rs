//! UTF-8 character processing benchmarks.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rmux_core::utf8::Utf8Char;

fn bench_from_ascii(c: &mut Criterion) {
    c.bench_function("utf8_from_ascii", |b| {
        b.iter(|| {
            for byte in b'!'..=b'~' {
                black_box(Utf8Char::from_ascii(byte));
            }
        });
    });
}

fn bench_from_char_ascii(c: &mut Criterion) {
    c.bench_function("utf8_from_char_ascii", |b| {
        b.iter(|| {
            for ch in '!'..='~' {
                black_box(Utf8Char::from_char(ch));
            }
        });
    });
}

fn bench_from_char_cjk(c: &mut Criterion) {
    let chars: Vec<char> = ('\u{4E00}'..'\u{4E00}').chain('\u{4E00}'..='\u{4EFF}').collect();
    c.bench_function("utf8_from_char_cjk_256", |b| {
        b.iter(|| {
            for &ch in &chars {
                black_box(Utf8Char::from_char(ch));
            }
        });
    });
}

fn bench_from_bytes(c: &mut Criterion) {
    let samples: Vec<Vec<u8>> = vec![
        b"A".to_vec(),
        "é".as_bytes().to_vec(),
        "世".as_bytes().to_vec(),
        "\u{1F600}".as_bytes().to_vec(),
    ];
    c.bench_function("utf8_from_bytes_mixed", |b| {
        b.iter(|| {
            for sample in &samples {
                black_box(Utf8Char::from_bytes(sample));
            }
        });
    });
}

fn bench_width_lookup(c: &mut Criterion) {
    let chars: Vec<Utf8Char> = (0x20u8..=0x7e)
        .map(Utf8Char::from_ascii)
        .chain(('\u{4E00}'..='\u{4EFF}').map(Utf8Char::from_char))
        .collect();
    c.bench_function("utf8_width_lookup_mixed", |b| {
        b.iter(|| {
            let mut total: u32 = 0;
            for ch in &chars {
                total += ch.width() as u32;
            }
            black_box(total);
        });
    });
}

criterion_group!(
    benches,
    bench_from_ascii,
    bench_from_char_ascii,
    bench_from_char_cjk,
    bench_from_bytes,
    bench_width_lookup
);
criterion_main!(benches);
