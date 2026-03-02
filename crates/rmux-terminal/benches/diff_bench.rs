//! Screen diff benchmarks.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use rmux_core::grid::cell::{CellFlags, GridCell};
use rmux_core::screen::Screen;
use rmux_core::style::Style;
use rmux_core::utf8::Utf8Char;
use rmux_terminal::output::diff::diff_screens;
use rmux_terminal::output::writer::TermWriter;

fn make_screen_with_content(width: u32, height: u32, ch: u8) -> Screen {
    let mut screen = Screen::new(width, height, 0);
    let cell = GridCell {
        data: Utf8Char::from_ascii(ch),
        style: Style::DEFAULT,
        link: 0,
        flags: CellFlags::empty(),
    };
    for y in 0..height {
        for x in 0..width {
            screen.grid.set_cell(x, y, &cell);
        }
    }
    screen
}

fn bench_diff_identical(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_identical_screens");

    for &(w, h) in &[(80, 24), (200, 50)] {
        let old = make_screen_with_content(w, h, b'A');
        let new = make_screen_with_content(w, h, b'A');

        group.bench_with_input(BenchmarkId::new("size", format!("{w}x{h}")), &(w, h), |b, _| {
            b.iter(|| {
                let mut writer = TermWriter::new(65536);
                diff_screens(black_box(&old), black_box(&new), &mut writer);
                black_box(writer.buffer());
            });
        });
    }
    group.finish();
}

fn bench_diff_completely_different(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_full_change");

    for &(w, h) in &[(80, 24), (200, 50)] {
        let old = make_screen_with_content(w, h, b'A');
        let new = make_screen_with_content(w, h, b'B');

        group.bench_with_input(BenchmarkId::new("size", format!("{w}x{h}")), &(w, h), |b, _| {
            b.iter(|| {
                let mut writer = TermWriter::new(65536);
                diff_screens(black_box(&old), black_box(&new), &mut writer);
                black_box(writer.buffer());
            });
        });
    }
    group.finish();
}

fn bench_diff_single_line_change(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_single_line");

    for &(w, h) in &[(80, 24), (200, 50)] {
        let old = make_screen_with_content(w, h, b'A');
        let mut new = make_screen_with_content(w, h, b'A');
        // Change only the middle line
        let mid = h / 2;
        let cell = GridCell {
            data: Utf8Char::from_ascii(b'X'),
            style: Style::DEFAULT,
            link: 0,
            flags: CellFlags::empty(),
        };
        for x in 0..w {
            new.grid.set_cell(x, mid, &cell);
        }

        group.bench_with_input(BenchmarkId::new("size", format!("{w}x{h}")), &(w, h), |b, _| {
            b.iter(|| {
                let mut writer = TermWriter::new(65536);
                diff_screens(black_box(&old), black_box(&new), &mut writer);
                black_box(writer.buffer());
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_diff_identical,
    bench_diff_completely_different,
    bench_diff_single_line_change,
);
criterion_main!(benches);
