//! Format string expansion benchmarks.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rmux_server::format::{FormatContext, format_expand};

fn make_context() -> FormatContext {
    let mut ctx = FormatContext::new();
    ctx.set("session_name", "main");
    ctx.set("session_id", "0");
    ctx.set("window_index", "1");
    ctx.set("window_name", "bash");
    ctx.set("pane_index", "0");
    ctx.set("pane_id", "%0");
    ctx.set("pane_width", "80");
    ctx.set("pane_height", "24");
    ctx.set("host", "myhost.example.com");
    ctx.set("host_short", "myhost");
    ctx
}

fn bench_simple_expansion(c: &mut Criterion) {
    let ctx = make_context();
    c.bench_function("format_simple", |b| {
        b.iter(|| {
            black_box(format_expand(black_box("#{session_name}"), &ctx));
        });
    });
}

fn bench_multiple_variables(c: &mut Criterion) {
    let ctx = make_context();
    c.bench_function("format_multi_vars", |b| {
        b.iter(|| {
            black_box(format_expand(
                black_box("[#{session_name}] #{window_index}:#{window_name} (#{pane_width}x#{pane_height})"),
                &ctx,
            ));
        });
    });
}

fn bench_no_variables(c: &mut Criterion) {
    let ctx = make_context();
    c.bench_function("format_plain_text", |b| {
        b.iter(|| {
            black_box(format_expand(black_box("just plain text with no variables"), &ctx));
        });
    });
}

fn bench_unknown_variable(c: &mut Criterion) {
    let ctx = make_context();
    c.bench_function("format_unknown_var", |b| {
        b.iter(|| {
            black_box(format_expand(black_box("#{nonexistent_var}"), &ctx));
        });
    });
}

criterion_group!(
    benches,
    bench_simple_expansion,
    bench_multiple_variables,
    bench_no_variables,
    bench_unknown_variable,
);
criterion_main!(benches);
