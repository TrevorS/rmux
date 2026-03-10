//! Grid line operation benchmarks.
//!
//! Measures hot-path line-level operations: clear_range, fill_to,
//! compact_extended, and set_cell with extended (Unicode) cells.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rmux_core::grid::cell::{CellFlags, GridCell};
use rmux_core::grid::line::GridLine;
use rmux_core::style::{Color, Style};
use rmux_core::utf8::Utf8Char;
use std::hint::black_box;

fn ascii_cell(ch: u8) -> GridCell {
    GridCell {
        data: Utf8Char::from_ascii(ch),
        style: Style::DEFAULT,
        link: 0,
        flags: CellFlags::empty(),
    }
}

fn unicode_cell(ch: char) -> GridCell {
    GridCell {
        data: Utf8Char::from_char(ch),
        style: Style { fg: Color::Palette(196), ..Style::DEFAULT },
        link: 0,
        flags: CellFlags::empty(),
    }
}

/// Benchmark clear_range (called on every erase-in-line, erase-in-display).
fn bench_clear_range(c: &mut Criterion) {
    let mut group = c.benchmark_group("line_clear_range");

    for &width in &[80u32, 200u32] {
        group.bench_with_input(BenchmarkId::new("full_line", width), &width, |b, &w| {
            let mut line = GridLine::with_capacity(w);
            for x in 0..w {
                line.set_cell(x, &ascii_cell(b'A' + (x % 26) as u8));
            }
            b.iter(|| {
                line.clear_range(black_box(0), black_box(w), Color::Default);
            });
        });

        group.bench_with_input(BenchmarkId::new("partial", width), &width, |b, &w| {
            let mut line = GridLine::with_capacity(w);
            for x in 0..w {
                line.set_cell(x, &ascii_cell(b'A' + (x % 26) as u8));
            }
            let start = w / 4;
            let end = w * 3 / 4;
            b.iter(|| {
                line.clear_range(black_box(start), black_box(end), Color::Default);
            });
        });
    }
    group.finish();
}

/// Benchmark fill_to (called when extending lines during cursor movement).
fn bench_fill_to(c: &mut Criterion) {
    let mut group = c.benchmark_group("line_fill_to");

    for &width in &[80u32, 200u32, 500u32] {
        group.bench_with_input(BenchmarkId::new("from_empty", width), &width, |b, &w| {
            b.iter_with_setup(GridLine::new, |mut line| {
                line.fill_to(w);
                black_box(&line);
            });
        });
    }
    group.finish();
}

/// Benchmark compact_extended (reclaims extended cell storage).
fn bench_compact_extended(c: &mut Criterion) {
    let mut group = c.benchmark_group("line_compact_extended");

    // Create a line with many extended cells, then overwrite some with ASCII
    for &(total, overwritten) in &[(50u32, 25u32), (200u32, 150u32)] {
        group.bench_with_input(
            BenchmarkId::new("cells", format!("{total}_overwrite_{overwritten}")),
            &(total, overwritten),
            |b, &(total, overwritten)| {
                b.iter_with_setup(
                    || {
                        let mut line = GridLine::new();
                        // Fill with Unicode cells (all use extended storage)
                        let cjk_chars = ['世', '界', '你', '好', '中', '文', '字', '体'];
                        for x in 0..total {
                            line.set_cell(
                                x,
                                &unicode_cell(cjk_chars[(x as usize) % cjk_chars.len()]),
                            );
                        }
                        // Overwrite some with ASCII (makes their extended entries orphaned)
                        for x in 0..overwritten {
                            line.set_cell(x, &ascii_cell(b'A'));
                        }
                        line
                    },
                    |mut line| {
                        line.compact_extended();
                        black_box(&line);
                    },
                );
            },
        );
    }
    group.finish();
}

/// Benchmark mixed ASCII/Unicode cell writes (realistic terminal content).
fn bench_mixed_content(c: &mut Criterion) {
    c.bench_function("line_mixed_ascii_unicode_80", |b| {
        let cjk = unicode_cell('世');
        let ascii = ascii_cell(b'A');
        b.iter(|| {
            let mut line = GridLine::new();
            for x in 0..80u32 {
                if x % 10 < 2 {
                    line.set_cell(x, black_box(&cjk));
                } else {
                    line.set_cell(x, black_box(&ascii));
                }
            }
            black_box(&line);
        });
    });
}

criterion_group!(
    benches,
    bench_clear_range,
    bench_fill_to,
    bench_compact_extended,
    bench_mixed_content,
);
criterion_main!(benches);
