//! Layout calculation benchmarks.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use rmux_core::layout::{LayoutCell, layout_even_horizontal, layout_even_vertical};

fn bench_even_horizontal(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_even_horizontal");

    for &n_panes in &[2, 4, 8, 16, 32] {
        let pane_ids: Vec<u32> = (0..n_panes).collect();
        group.bench_with_input(BenchmarkId::new("panes", n_panes), &pane_ids, |b, ids| {
            b.iter(|| {
                black_box(layout_even_horizontal(200, 50, ids));
            });
        });
    }
    group.finish();
}

fn bench_even_vertical(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_even_vertical");

    for &n_panes in &[2, 4, 8, 16] {
        let pane_ids: Vec<u32> = (0..n_panes).collect();
        group.bench_with_input(BenchmarkId::new("panes", n_panes), &pane_ids, |b, ids| {
            b.iter(|| {
                black_box(layout_even_vertical(200, 50, ids));
            });
        });
    }
    group.finish();
}

fn bench_split_horizontal(c: &mut Criterion) {
    c.bench_function("layout_split_horizontal", |b| {
        b.iter(|| {
            let mut cell = LayoutCell::new_pane(0, 0, 200, 50, 0);
            for i in 1..10 {
                if let Some(pane) = cell.find_pane(i - 1) {
                    let _ = pane;
                }
                // Split the first leaf we find
                split_first_leaf(&mut cell, i);
            }
            black_box(&cell);
        });
    });
}

fn split_first_leaf(cell: &mut LayoutCell, new_id: u32) {
    if cell.is_pane() && cell.sx >= 3 {
        cell.split_horizontal(new_id);
        return;
    }
    for child in &mut cell.children {
        if child.is_pane() && child.sx >= 3 {
            child.split_horizontal(new_id);
            return;
        }
    }
}

fn bench_find_pane(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_find_pane");

    for &n_panes in &[4, 16, 64] {
        let pane_ids: Vec<u32> = (0..n_panes).collect();
        let layout = layout_even_horizontal(800, 50, &pane_ids);
        let target = n_panes - 1; // Find last pane

        group.bench_with_input(BenchmarkId::new("panes", n_panes), &target, |b, &target| {
            b.iter(|| {
                black_box(layout.find_pane(target));
            });
        });
    }
    group.finish();
}

fn bench_pane_at(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_pane_at");

    for &n_panes in &[4, 16, 64] {
        let pane_ids: Vec<u32> = (0..n_panes).collect();
        let layout = layout_even_horizontal(800, 50, &pane_ids);

        group.bench_with_input(BenchmarkId::new("panes", n_panes), &n_panes, |b, _| {
            b.iter(|| {
                // Look up middle of screen
                black_box(layout.pane_at(400, 25));
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_even_horizontal,
    bench_even_vertical,
    bench_split_horizontal,
    bench_find_pane,
    bench_pane_at,
);
criterion_main!(benches);
