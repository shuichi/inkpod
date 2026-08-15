//! Private pre-ratification InkScript compilation, binding, and staged execution.

#![allow(
    dead_code,
    reason = "the compiler and staged runner remain private until catalog ratification"
)]

mod assets;
pub(crate) mod bind;
mod catalog;
mod compile;
mod execute;
#[cfg(test)]
mod performance;
mod plan;
mod report;
mod run;

#[allow(
    unused_imports,
    reason = "the private compiler surface is exercised by colocated tests"
)]
pub(crate) use compile::{
    ScriptCompileError, ScriptCompileLimits, StaticScriptProgram, compile_inkscript,
    compile_inkscript_with_limits,
};
#[allow(
    unused_imports,
    reason = "the private runner surface is exercised by colocated tests"
)]
pub(crate) use execute::{
    CapturedScriptInput, ScriptDryRunResult, ScriptRunError, capture_in_memory_fingerprint,
    capture_in_memory_input, capture_in_memory_input_at, native_script_input, run_inkscript_dry,
};

#[cfg(test)]
mod tests;
