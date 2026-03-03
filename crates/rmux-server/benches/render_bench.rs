//! Render benchmarks.
//!
//! Measures the performance of rendering windows with various pane configurations.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rmux_core::layout::{LayoutCell, LayoutType, layout_even_horizontal};
use rmux_server::pane::Pane;
use rmux_server::render::{WindowInfo, render_window};
use rmux_server::window::Window;
use std::hint::black_box;

/// Create a window with a single pane of the given dimensions.
fn make_single_pane_window(sx: u32, sy: u32) -> Window {
    // sy is the total height including the status line row,
    // pane height is sy - 1 to leave room for the status line.
    let pane_h = sy.saturating_sub(1);
    let mut window = Window::new("bench".into(), sx, pane_h);
    let pane = Pane::new(sx, pane_h, 0);
    let pid = pane.id;
    window.active_pane = pid;
    window.panes.insert(pid, pane);
    window
}

/// Create a window with 2 horizontal panes.
fn make_two_pane_horizontal_window(sx: u32, sy: u32) -> Window {
    let pane_h = sy.saturating_sub(1);
    let half = sx / 2;
    let right_width = sx - half - 1; // -1 for separator

    let mut window = Window::new("bench2".into(), sx, pane_h);
    let mut pane1 = Pane::new(half, pane_h, 0);
    let mut pane2 = Pane::new(right_width, pane_h, 0);
    pane1.xoff = 0;
    pane1.yoff = 0;
    pane2.xoff = half + 1;
    pane2.yoff = 0;

    let pid1 = pane1.id;
    let pid2 = pane2.id;
    window.active_pane = pid1;
    window.panes.insert(pid1, pane1);
    window.panes.insert(pid2, pane2);
    window.layout = Some(layout_even_horizontal(sx, pane_h, &[pid1, pid2]));
    window
}

/// Create a window with a 4-pane grid (2x2).
fn make_four_pane_grid_window(sx: u32, sy: u32) -> Window {
    let pane_h = sy.saturating_sub(1);
    let half_w = sx / 2;
    let right_width = sx - half_w - 1;
    let half_h = pane_h / 2;
    let bottom_height = pane_h - half_h - 1;

    let mut window = Window::new("bench4".into(), sx, pane_h);

    let mut pane1 = Pane::new(half_w, half_h, 0);
    let mut pane2 = Pane::new(right_width, half_h, 0);
    let mut pane3 = Pane::new(half_w, bottom_height, 0);
    let mut pane4 = Pane::new(right_width, bottom_height, 0);

    pane1.xoff = 0;
    pane1.yoff = 0;
    pane2.xoff = half_w + 1;
    pane2.yoff = 0;
    pane3.xoff = 0;
    pane3.yoff = half_h + 1;
    pane4.xoff = half_w + 1;
    pane4.yoff = half_h + 1;

    let pid1 = pane1.id;
    let pid2 = pane2.id;
    let pid3 = pane3.id;
    let pid4 = pane4.id;

    window.active_pane = pid1;
    window.panes.insert(pid1, pane1);
    window.panes.insert(pid2, pane2);
    window.panes.insert(pid3, pane3);
    window.panes.insert(pid4, pane4);

    // Build a 2x2 grid layout: top-bottom split where each child is a left-right split
    let top_row = layout_even_horizontal(sx, half_h, &[pid1, pid2]);
    let mut bottom_row = layout_even_horizontal(sx, bottom_height, &[pid3, pid4]);
    // Adjust y offsets for the bottom row
    bottom_row.y_off = half_h + 1;
    for child in &mut bottom_row.children {
        child.y_off = half_h + 1;
    }

    let mut root = LayoutCell::new_split(LayoutType::TopBottom, 0, 0, sx, pane_h);
    root.children.push(top_row);
    root.children.push(bottom_row);
    window.layout = Some(root);

    window
}

fn bench_window_list(name: &str) -> Vec<WindowInfo> {
    vec![WindowInfo { idx: 0, name: name.to_string(), is_active: true }]
}

fn bench_render_single_pane(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_single_pane");

    for &(sx, sy) in &[(80u32, 24u32), (200u32, 50u32)] {
        let window = make_single_pane_window(sx, sy);
        let wl = bench_window_list("bench");
        group.bench_with_input(
            BenchmarkId::new("size", format!("{sx}x{sy}")),
            &(sx, sy),
            |b, &(sx, sy)| {
                b.iter(|| {
                    black_box(render_window(
                        black_box(&window),
                        black_box("main"),
                        black_box(sx),
                        black_box(sy),
                        black_box(&wl),
                        None,
                    ));
                });
            },
        );
    }
    group.finish();
}

fn bench_render_two_pane_horizontal(c: &mut Criterion) {
    let window = make_two_pane_horizontal_window(80, 24);
    let wl = bench_window_list("bench2");
    c.bench_function("render_2pane_horizontal_80x24", |b| {
        b.iter(|| {
            black_box(render_window(
                black_box(&window),
                black_box("main"),
                black_box(80),
                black_box(24),
                black_box(&wl),
                None,
            ));
        });
    });
}

fn bench_render_four_pane_grid(c: &mut Criterion) {
    let window = make_four_pane_grid_window(80, 24);
    let wl = bench_window_list("bench4");
    c.bench_function("render_4pane_grid_80x24", |b| {
        b.iter(|| {
            black_box(render_window(
                black_box(&window),
                black_box("main"),
                black_box(80),
                black_box(24),
                black_box(&wl),
                None,
            ));
        });
    });
}

criterion_group!(
    benches,
    bench_render_single_pane,
    bench_render_two_pane_horizontal,
    bench_render_four_pane_grid,
);
criterion_main!(benches);
