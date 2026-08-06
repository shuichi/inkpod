#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let _ = inkpod_format::decode_procedure_file(bytes);
});
