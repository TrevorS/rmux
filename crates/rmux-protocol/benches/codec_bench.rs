//! Protocol codec benchmarks.
//!
//! Measures the performance of encoding and decoding protocol messages.

use bytes::BytesMut;
use criterion::{Criterion, criterion_group, criterion_main};
use rmux_protocol::codec::{decode_message, encode_message};
use rmux_protocol::message::{Message, MsgCommand, PROTOCOL_VERSION};
use std::hint::black_box;

fn make_test_messages() -> Vec<(&'static str, Message)> {
    vec![
        ("Version", Message::Version { version: PROTOCOL_VERSION }),
        ("Resize", Message::Resize { sx: 120, sy: 40, xpixel: 960, ypixel: 640 }),
        (
            "Command",
            Message::Command(MsgCommand {
                argc: 3,
                argv: vec!["new-session".to_string(), "-s".to_string(), "main".to_string()],
            }),
        ),
        (
            "OutputData",
            Message::OutputData(
                b"Hello, world! \x1b[31mred text\x1b[0m and more output data here".to_vec(),
            ),
        ),
        ("InputData", Message::InputData(vec![0x1b, b'[', b'A', b'h', b'e', b'l', b'l', b'o'])),
        ("IdentifyFlags", Message::IdentifyFlags(0x1234_5678)),
    ]
}

fn bench_encode(c: &mut Criterion) {
    let messages = make_test_messages();

    let mut group = c.benchmark_group("encode_message");
    for (name, msg) in &messages {
        group.bench_function(*name, |b| {
            let mut buf = BytesMut::with_capacity(256);
            b.iter(|| {
                buf.clear();
                black_box(encode_message(black_box(msg), &mut buf).unwrap());
            });
        });
    }
    group.finish();
}

fn bench_decode(c: &mut Criterion) {
    let messages = make_test_messages();

    let mut group = c.benchmark_group("decode_message");
    for (name, msg) in &messages {
        // Pre-encode the message to get the raw bytes
        let mut encoded = BytesMut::new();
        encode_message(msg, &mut encoded).unwrap();
        let raw = encoded.to_vec();

        group.bench_function(*name, |b| {
            b.iter(|| {
                let mut buf = BytesMut::from(raw.as_slice());
                black_box(decode_message(&mut buf).unwrap());
            });
        });
    }
    group.finish();
}

fn bench_roundtrip(c: &mut Criterion) {
    let messages = make_test_messages();

    let mut group = c.benchmark_group("roundtrip");
    for (name, msg) in &messages {
        group.bench_function(*name, |b| {
            let mut buf = BytesMut::with_capacity(256);
            b.iter(|| {
                buf.clear();
                encode_message(black_box(msg), &mut buf).unwrap();
                let decoded = decode_message(&mut buf).unwrap();
                black_box(decoded);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_encode, bench_decode, bench_roundtrip,);
criterion_main!(benches);
