#![no_main]

use inkpod_format::{InkScriptSource, InkScriptSourceId, lex_inkscript};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(source) = InkScriptSource::new(InkScriptSourceId::new(0), bytes) {
        let _ = lex_inkscript(&source);
    }
});
