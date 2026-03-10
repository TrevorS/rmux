//! Options lookup benchmarks.
//!
//! Measures the performance of option lookups through the parent chain,
//! which happens on nearly every server operation.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rmux_core::options::{
    OptionValue, Options, default_server_options, default_session_options, default_window_options,
};
use std::hint::black_box;

/// Benchmark direct local option lookup (no parent chain).
fn bench_local_lookup(c: &mut Criterion) {
    let mut opts = Options::new();
    opts.set("status", OptionValue::Flag(true));
    opts.set("status-style", OptionValue::String("bg=green".into()));
    opts.set("history-limit", OptionValue::Number(2000));

    c.bench_function("options_local_lookup", |b| {
        b.iter(|| {
            black_box(opts.get(black_box("status")));
            black_box(opts.get(black_box("status-style")));
            black_box(opts.get(black_box("history-limit")));
        });
    });
}

/// Benchmark lookup that falls through to parent (the common case:
/// session inherits from server defaults).
fn bench_parent_chain_lookup(c: &mut Criterion) {
    let server = default_server_options();
    let session = Options::with_parent(server);
    let window = Options::with_parent(default_window_options());

    let mut group = c.benchmark_group("options_parent_chain");

    group.bench_function("session_inherits_server", |b| {
        b.iter(|| {
            // These should all fall through to the server parent
            let _ = black_box(session.get_string(black_box("default-terminal")));
            let _ = black_box(session.get_number(black_box("escape-time")));
            let _ = black_box(session.get_flag(black_box("exit-empty")));
        });
    });

    group.bench_function("window_local", |b| {
        b.iter(|| {
            let _ = black_box(window.get_flag(black_box("aggressive-resize")));
            let _ = black_box(window.get_string(black_box("mode-keys")));
        });
    });

    group.finish();
}

/// Benchmark the full 3-level chain: window → session → server.
fn bench_deep_chain(c: &mut Criterion) {
    let server = default_server_options();
    let session = Options::with_parent(server);
    let window = Options::with_parent(session);

    c.bench_function("options_deep_chain_3_levels", |b| {
        b.iter(|| {
            // Lookup that goes through window → session → server
            black_box(window.get(black_box("default-terminal")));
            black_box(window.get(black_box("escape-time")));
            // Miss: key doesn't exist at any level
            black_box(window.get(black_box("nonexistent-option")));
        });
    });
}

/// Benchmark option set (write path).
fn bench_set(c: &mut Criterion) {
    let mut group = c.benchmark_group("options_set");

    group.bench_function("string", |b| {
        let mut opts = Options::new();
        b.iter(|| {
            opts.set(black_box("status-left"), OptionValue::String("test".into()));
        });
    });

    group.bench_function("number", |b| {
        let mut opts = Options::new();
        b.iter(|| {
            opts.set(black_box("history-limit"), OptionValue::Number(50000));
        });
    });

    group.finish();
}

/// Benchmark parse_and_set (string → typed value conversion).
fn bench_parse_and_set(c: &mut Criterion) {
    let mut group = c.benchmark_group("options_parse_and_set");

    let cases: &[(&str, &str)] = &[
        ("history-limit", "50000"),
        ("status", "on"),
        ("default-terminal", "xterm-256color"),
        ("escape-time", "500"),
    ];

    for &(key, value) in cases {
        group.bench_with_input(BenchmarkId::new("parse", key), &(key, value), |b, &(k, v)| {
            let mut opts = Options::new();
            b.iter(|| {
                opts.parse_and_set(black_box(k), black_box(v));
            });
        });
    }

    group.finish();
}

/// Benchmark all_entries() which collects the full option hierarchy.
fn bench_all_entries(c: &mut Criterion) {
    let server = default_server_options();
    let session = default_session_options();
    let window = Options::with_parent(session);

    let mut group = c.benchmark_group("options_all_entries");

    group.bench_function("server_flat", |b| {
        b.iter(|| {
            black_box(server.all_entries());
        });
    });

    group.bench_function("window_with_parent", |b| {
        b.iter(|| {
            black_box(window.all_entries());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_local_lookup,
    bench_parent_chain_lookup,
    bench_deep_chain,
    bench_set,
    bench_parse_and_set,
    bench_all_entries,
);
criterion_main!(benches);
