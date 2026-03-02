//! Input parser benchmarks.
//!
//! Measures throughput for various types of terminal output:
//! pure ASCII, colored text, escape-heavy output.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use rmux_core::screen::Screen;
use rmux_terminal::input::InputParser;

fn make_ascii_data(size: usize) -> Vec<u8> {
    let line: Vec<u8> = (b'!'..=b'~')
        .cycle()
        .take(79)
        .chain(std::iter::once(b'\r'))
        .chain(std::iter::once(b'\n'))
        .collect();
    line.iter().cycle().take(size).copied().collect()
}

fn make_colored_data(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size);
    let colors = [31, 32, 33, 34, 35, 36]; // Red through cyan
    let mut color_idx = 0;
    let mut produced = 0;

    while produced < size {
        // Color escape
        let esc = format!("\x1b[{}m", colors[color_idx % colors.len()]);
        data.extend_from_slice(esc.as_bytes());
        produced += esc.len();

        // Some text
        let text = b"Hello World ";
        data.extend_from_slice(text);
        produced += text.len();

        color_idx += 1;
        if produced % 800 < 80 {
            data.extend_from_slice(b"\r\n");
            produced += 2;
        }
    }
    data.truncate(size);
    data
}

fn make_escape_heavy_data(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size);
    let mut produced = 0;

    while produced < size {
        // Simulate colored diff output
        // Move cursor, set color, print, reset
        let chunk = "\x1b[1;32m+\x1b[0m line content here with some text\x1b[K\r\n\
             \x1b[1;31m-\x1b[0m old line removed from the file\x1b[K\r\n\
             \x1b[36m@@\x1b[0m -10,5 +10,7 @@ function_name\r\n"
            .to_string();
        data.extend_from_slice(chunk.as_bytes());
        produced += chunk.len();
    }
    data.truncate(size);
    data
}

fn bench_pure_ascii(c: &mut Criterion) {
    let mut group = c.benchmark_group("input_parse_ascii");

    for &size in &[1024, 65536, 1_048_576] {
        let data = make_ascii_data(size);
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(format!("{size}")), &data, |b, data| {
            b.iter(|| {
                let mut screen = Screen::new(80, 24, 0);
                let mut parser = InputParser::new();
                parser.parse(black_box(data), &mut screen);
            });
        });
    }
    group.finish();
}

fn bench_colored_text(c: &mut Criterion) {
    let mut group = c.benchmark_group("input_parse_colored");

    for &size in &[1024, 65536, 1_048_576] {
        let data = make_colored_data(size);
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(format!("{size}")), &data, |b, data| {
            b.iter(|| {
                let mut screen = Screen::new(80, 24, 0);
                let mut parser = InputParser::new();
                parser.parse(black_box(data), &mut screen);
            });
        });
    }
    group.finish();
}

fn bench_escape_heavy(c: &mut Criterion) {
    let mut group = c.benchmark_group("input_parse_escape_heavy");

    for &size in &[1024, 65536, 1_048_576] {
        let data = make_escape_heavy_data(size);
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(format!("{size}")), &data, |b, data| {
            b.iter(|| {
                let mut screen = Screen::new(200, 50, 0);
                let mut parser = InputParser::new();
                parser.parse(black_box(data), &mut screen);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_pure_ascii, bench_colored_text, bench_escape_heavy);
criterion_main!(benches);
