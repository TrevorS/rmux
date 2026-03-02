//! Grid operations benchmarks.
//!
//! These benchmarks measure the hot-path operations on the grid:
//! cell read/write, scroll, history management.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use rmux_core::grid::Grid;
use rmux_core::grid::cell::{CellFlags, GridCell};
use rmux_core::style::{Attrs, Color, Style};
use rmux_core::utf8::Utf8Char;

fn make_ascii_cell(ch: u8) -> GridCell {
    GridCell {
        data: Utf8Char::from_ascii(ch),
        style: Style::DEFAULT,
        link: 0,
        flags: CellFlags::empty(),
    }
}

fn make_colored_cell(ch: u8) -> GridCell {
    GridCell {
        data: Utf8Char::from_ascii(ch),
        style: Style {
            fg: Color::Palette(196),
            bg: Color::Palette(16),
            us: Color::Default,
            attrs: Attrs::BOLD,
        },
        link: 0,
        flags: CellFlags::empty(),
    }
}

fn bench_set_cell(c: &mut Criterion) {
    let mut group = c.benchmark_group("grid_set_cell");

    for &(cols, rows) in &[(80, 24), (200, 50), (300, 80)] {
        group.bench_with_input(
            BenchmarkId::new("ascii", format!("{cols}x{rows}")),
            &(cols, rows),
            |b, &(cols, rows)| {
                let mut grid = Grid::new(cols, rows, 0);
                let cell = make_ascii_cell(b'A');
                b.iter(|| {
                    for y in 0..rows {
                        for x in 0..cols {
                            grid.set_cell(x, y, black_box(&cell));
                        }
                    }
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("colored", format!("{cols}x{rows}")),
            &(cols, rows),
            |b, &(cols, rows)| {
                let mut grid = Grid::new(cols, rows, 0);
                let cell = make_colored_cell(b'X');
                b.iter(|| {
                    for y in 0..rows {
                        for x in 0..cols {
                            grid.set_cell(x, y, black_box(&cell));
                        }
                    }
                });
            },
        );
    }
    group.finish();
}

fn bench_get_cell(c: &mut Criterion) {
    let mut group = c.benchmark_group("grid_get_cell");

    for &(cols, rows) in &[(80, 24), (200, 50)] {
        let mut grid = Grid::new(cols, rows, 0);
        // Fill with data
        for y in 0..rows {
            for x in 0..cols {
                grid.set_cell(x, y, &make_ascii_cell(b'A' + (x % 26) as u8));
            }
        }

        group.bench_with_input(
            BenchmarkId::new("filled", format!("{cols}x{rows}")),
            &(cols, rows),
            |b, &(cols, rows)| {
                b.iter(|| {
                    let mut sum = 0u32;
                    for y in 0..rows {
                        for x in 0..cols {
                            let cell = grid.get_cell(black_box(x), black_box(y));
                            sum += cell.data.width() as u32;
                        }
                    }
                    black_box(sum);
                });
            },
        );
    }
    group.finish();
}

fn bench_scroll_up(c: &mut Criterion) {
    let mut group = c.benchmark_group("grid_scroll_up");

    for &history_limit in &[2_000, 50_000, 1_000_000] {
        group.bench_with_input(
            BenchmarkId::new("scroll", format!("limit_{history_limit}")),
            &history_limit,
            |b, &limit| {
                let mut grid = Grid::new(80, 24, limit);
                b.iter(|| {
                    grid.scroll_up();
                });
            },
        );
    }
    group.finish();
}

fn bench_scroll_region(c: &mut Criterion) {
    let mut group = c.benchmark_group("grid_scroll_region");

    group.bench_function("region_5_20", |b| {
        let mut grid = Grid::new(80, 24, 0);
        // Fill with data
        for y in 0..24 {
            for x in 0..80 {
                grid.set_cell(x, y, &make_ascii_cell(b'A'));
            }
        }
        b.iter(|| {
            grid.scroll_region_up(black_box(5), black_box(20));
        });
    });

    group.finish();
}

fn bench_history_collect(c: &mut Criterion) {
    c.bench_function("history_collect_100k", |b| {
        b.iter_with_setup(
            || {
                let mut grid = Grid::new(80, 24, 10_000);
                // Fill up history past the limit
                for _ in 0..11_000 {
                    grid.scroll_up();
                }
                grid
            },
            |mut grid| {
                grid.collect_history();
                black_box(&grid);
            },
        );
    });
}

fn bench_grid_clear(c: &mut Criterion) {
    c.bench_function("grid_clear_80x24", |b| {
        let mut grid = Grid::new(80, 24, 0);
        for y in 0..24 {
            for x in 0..80 {
                grid.set_cell(x, y, &make_ascii_cell(b'X'));
            }
        }
        b.iter(|| {
            grid.clear();
        });
    });
}

criterion_group!(
    benches,
    bench_set_cell,
    bench_get_cell,
    bench_scroll_up,
    bench_scroll_region,
    bench_history_collect,
    bench_grid_clear
);
criterion_main!(benches);
