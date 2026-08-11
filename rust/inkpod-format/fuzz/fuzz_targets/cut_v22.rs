#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(descriptor) = inkpod_format::decode_cut_descriptor(bytes) {
        let _ = inkpod_format::encode_cut_descriptor(&descriptor);
    }
});
