#![no_main]
use libfuzzer_sys::fuzz_target;
use bytes::BytesMut;
use rmux_protocol::codec::decode_message;

fuzz_target!(|data: &[u8]| {
    let mut buf = BytesMut::from(data);
    // Decoder should return Ok(None), Ok(Some(msg)), or Err without panicking
    let _ = decode_message(&mut buf);
});
