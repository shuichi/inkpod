#![no_main]

use inkpod_format::{InkScriptSource, InkScriptSourceId, parse_inkscript};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(source) = InkScriptSource::new(InkScriptSourceId::new(0), bytes) {
        let parsed = parse_inkscript(&source);
        let mut written = Vec::new();
        parsed
            .cst()
            .write_lossless(&mut written)
            .expect("Vec writes cannot fail");
        assert_eq!(written, bytes);
    }
});
