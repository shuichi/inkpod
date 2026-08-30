#![no_main]

// Exercises the exact-current v32 native container codec.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(mut file) = inkpod_format::decode_procedure_file(bytes) {
        let _ = inkpod_format::encode_procedure_file(&file);
        file.sections
            .retain(|section| section.fourcc != *b"CKPT");
        let _ = inkpod_format::encode_procedure_file(&file);
    }
});
