//! Key parsing benchmarks.
//!
//! Measures the performance of parsing raw byte sequences into key codes
//! and converting key names back to byte sequences.

use criterion::{Criterion, criterion_group, criterion_main};
use rmux_terminal::keys::{key_name_to_bytes, parse_key_event};
use std::hint::black_box;

fn bench_parse_ascii(c: &mut Criterion) {
    c.bench_function("parse_key_event_ascii", |b| {
        b.iter(|| {
            for ch in b'a'..=b'z' {
                black_box(parse_key_event(black_box(&[ch])));
            }
            for ch in b'A'..=b'Z' {
                black_box(parse_key_event(black_box(&[ch])));
            }
            for ch in b'0'..=b'9' {
                black_box(parse_key_event(black_box(&[ch])));
            }
        });
    });
}

fn bench_parse_escape_sequences(c: &mut Criterion) {
    let sequences: &[&[u8]] = &[
        // Arrow keys (CSI)
        b"\x1b[A", // Up
        b"\x1b[B", // Down
        b"\x1b[C", // Right
        b"\x1b[D", // Left
        // Home/End (CSI)
        b"\x1b[H", // Home
        b"\x1b[F", // End
        // Function keys (SS3)
        b"\x1bOP", // F1
        b"\x1bOQ", // F2
        b"\x1bOR", // F3
        b"\x1bOS", // F4
        // Function keys (CSI ~)
        b"\x1b[15~", // F5
        b"\x1b[17~", // F6
        b"\x1b[18~", // F7
        b"\x1b[24~", // F12
        // Special keys
        b"\x1b[2~", // Insert
        b"\x1b[3~", // Delete
        b"\x1b[5~", // PageUp
        b"\x1b[6~", // PageDown
    ];

    c.bench_function("parse_key_event_escape_sequences", |b| {
        b.iter(|| {
            for seq in sequences {
                black_box(parse_key_event(black_box(seq)));
            }
        });
    });
}

fn bench_parse_utf8(c: &mut Criterion) {
    let sequences: &[&[u8]] = &[
        // 2-byte UTF-8: Latin characters with diacritics
        "\u{00e9}".as_bytes(), // e-acute
        "\u{00f1}".as_bytes(), // n-tilde
        "\u{00fc}".as_bytes(), // u-umlaut
        // 3-byte UTF-8: CJK and other scripts
        "\u{4e16}".as_bytes(), // Chinese character
        "\u{2603}".as_bytes(), // Snowman
        "\u{20ac}".as_bytes(), // Euro sign
        // 4-byte UTF-8: Emoji and supplementary
        "\u{1f600}".as_bytes(), // Grinning face
        "\u{1f4a9}".as_bytes(), // Pile of poo
        "\u{1f680}".as_bytes(), // Rocket
    ];

    c.bench_function("parse_key_event_utf8", |b| {
        b.iter(|| {
            for seq in sequences {
                black_box(parse_key_event(black_box(seq)));
            }
        });
    });
}

fn bench_key_name_to_bytes(c: &mut Criterion) {
    let names: &[&str] = &[
        "Enter", "Escape", "Space", "Tab", "BSpace", "Up", "Down", "Left", "Right", "Home", "End",
        "PPage", "NPage", "DC", "IC", "F1", "F2", "F5", "F12", "C-c", "C-a", "C-z", "M-x", "M-a",
        "a", "Z",
    ];

    c.bench_function("key_name_to_bytes", |b| {
        b.iter(|| {
            for name in names {
                black_box(key_name_to_bytes(black_box(name)));
            }
        });
    });
}

criterion_group!(
    benches,
    bench_parse_ascii,
    bench_parse_escape_sequences,
    bench_parse_utf8,
    bench_key_name_to_bytes,
);
criterion_main!(benches);
