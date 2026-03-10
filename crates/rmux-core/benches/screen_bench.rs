//! Screen operation benchmarks.
//!
//! Measures hot-path screen operations: cell writes with cursor advance,
//! resize, alternate screen switching, and scroll within regions.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rmux_core::grid::cell::{CellFlags, GridCell};
use rmux_core::screen::Screen;
use rmux_core::style::{Attrs, Color, Style};
use rmux_core::utf8::Utf8Char;
use std::hint::black_box;

fn make_cell(ch: u8) -> GridCell {
    GridCell {
        data: Utf8Char::from_ascii(ch),
        style: Style::DEFAULT,
        link: 0,
        flags: CellFlags::empty(),
    }
}

fn make_styled_cell(ch: u8) -> GridCell {
    GridCell {
        data: Utf8Char::from_ascii(ch),
        style: Style {
            fg: Color::Palette(196),
            bg: Color::Palette(16),
            us: Color::Default,
            attrs: Attrs::BOLD | Attrs::ITALICS,
        },
        link: 0,
        flags: CellFlags::empty(),
    }
}

/// Simulates writing a full screen of characters (the core of the render path).
fn bench_write_full_screen(c: &mut Criterion) {
    let mut group = c.benchmark_group("screen_write_full");

    for &(w, h) in &[(80u32, 24u32), (200u32, 50u32)] {
        let cell = make_cell(b'A');
        group.bench_with_input(
            BenchmarkId::new("ascii", format!("{w}x{h}")),
            &(w, h),
            |b, &(w, h)| {
                let mut screen = Screen::new(w, h, 0);
                b.iter(|| {
                    for y in 0..h {
                        for x in 0..w {
                            screen.grid.set_cell(x, y, black_box(&cell));
                        }
                    }
                    black_box(&screen);
                });
            },
        );

        let styled = make_styled_cell(b'X');
        group.bench_with_input(
            BenchmarkId::new("styled", format!("{w}x{h}")),
            &(w, h),
            |b, &(w, h)| {
                let mut screen = Screen::new(w, h, 0);
                b.iter(|| {
                    for y in 0..h {
                        for x in 0..w {
                            screen.grid.set_cell(x, y, black_box(&styled));
                        }
                    }
                    black_box(&screen);
                });
            },
        );
    }
    group.finish();
}

/// Benchmark screen resize (common during terminal window resizing).
fn bench_resize(c: &mut Criterion) {
    let mut group = c.benchmark_group("screen_resize");

    let transitions: &[(u32, u32, u32, u32)] =
        &[(80, 24, 200, 50), (200, 50, 80, 24), (80, 24, 120, 36)];

    for &(from_w, from_h, to_w, to_h) in transitions {
        group.bench_with_input(
            BenchmarkId::new("transition", format!("{from_w}x{from_h}_to_{to_w}x{to_h}")),
            &(from_w, from_h, to_w, to_h),
            |b, &(fw, fh, tw, th)| {
                b.iter_with_setup(
                    || {
                        let mut s = Screen::new(fw, fh, 2000);
                        // Fill with content
                        let cell = make_cell(b'A');
                        for y in 0..fh {
                            for x in 0..fw {
                                s.grid.set_cell(x, y, &cell);
                            }
                        }
                        s
                    },
                    |mut s| {
                        s.resize(tw, th);
                        black_box(&s);
                    },
                );
            },
        );
    }
    group.finish();
}

/// Benchmark alternate screen enter/exit (used by vim, less, htop, etc.).
fn bench_alternate_screen(c: &mut Criterion) {
    let mut group = c.benchmark_group("screen_alternate");

    for &(w, h) in &[(80u32, 24u32), (200u32, 50u32)] {
        group.bench_with_input(
            BenchmarkId::new("enter_exit", format!("{w}x{h}")),
            &(w, h),
            |b, &(w, h)| {
                b.iter_with_setup(
                    || {
                        let mut s = Screen::new(w, h, 2000);
                        let cell = make_cell(b'A');
                        for y in 0..h {
                            for x in 0..w {
                                s.grid.set_cell(x, y, &cell);
                            }
                        }
                        s
                    },
                    |mut s| {
                        s.enter_alternate();
                        s.exit_alternate();
                        black_box(&s);
                    },
                );
            },
        );
    }
    group.finish();
}

/// Benchmark scroll-up with history (the most frequent grid mutation).
fn bench_scroll_with_history(c: &mut Criterion) {
    let mut group = c.benchmark_group("screen_scroll_history");

    for &limit in &[2_000u32, 50_000u32] {
        group.bench_with_input(BenchmarkId::new("limit", limit), &limit, |b, &limit| {
            let mut screen = Screen::new(80, 24, limit);
            let cell = make_cell(b'A');
            for y in 0..24 {
                for x in 0..80 {
                    screen.grid.set_cell(x, y, &cell);
                }
            }
            b.iter(|| {
                screen.grid.scroll_up();
                black_box(&screen);
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_write_full_screen,
    bench_resize,
    bench_alternate_screen,
    bench_scroll_with_history,
);
criterion_main!(benches);
