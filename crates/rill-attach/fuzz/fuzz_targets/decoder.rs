#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_attach::Decoder;

fuzz_target!(|data: &[u8]| {
    let mut decoder = Decoder::new();
    let _ = decoder.push(data);
});
