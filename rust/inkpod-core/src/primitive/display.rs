//! Deterministic, bounded presentation of canonical procedure arguments.

use super::catalog::primitive_name;
use super::digest::decode_color;
use super::executor::{decode_color_chart, decode_palette};
use super::*;
use crate::CoreError;
use std::fmt::Write;

const MAX_INLINE_ARGUMENT_BYTES: usize = 16 * 1024;
const MAX_INLINE_DEBUG_CHARS: usize = 16 * 1024;

pub(crate) fn display_procedure(
    procedure: &CanonicalProcedure,
) -> Result<(String, String), CoreError> {
    let name = primitive_name(procedure.primitive_id())
        .ok_or(CoreError::InvalidState(
            "journal primitive is not in the catalog",
        ))?
        .to_owned();
    let mut arguments = match procedure.primitive_id() {
        PrimitiveId::SET_MAIN_LINE_COLOR => {
            format!("color={:?}", decode_color(procedure.canonical_arguments())?)
        }
        PrimitiveId::REPLACE_PALETTE => {
            let colors = decode_palette(procedure.canonical_arguments())?;
            if procedure.canonical_arguments().len() <= MAX_INLINE_ARGUMENT_BYTES {
                format!("colors_count={}, colors={colors:?}", colors.len())
            } else {
                format!(
                    "colors_count={}, {}",
                    colors.len(),
                    byte_summary("canonical_arguments", procedure.canonical_arguments())
                )
            }
        }
        PrimitiveId::REPLACE_COLOR_CHART => {
            let (entries, locked) = decode_color_chart(procedure.canonical_arguments())?;
            if procedure.canonical_arguments().len() <= MAX_INLINE_ARGUMENT_BYTES {
                format!(
                    "locked={locked}, entries_count={}, entries={entries:?}",
                    entries.len()
                )
            } else {
                format!(
                    "locked={locked}, entries_count={}, {}",
                    entries.len(),
                    byte_summary("canonical_arguments", procedure.canonical_arguments())
                )
            }
        }
        _ if procedure.canonical_arguments().len() > MAX_INLINE_ARGUMENT_BYTES => {
            byte_summary("canonical_arguments", procedure.canonical_arguments())
        }
        _ => procedure.runtime_invocation.as_ref().map_or_else(
            || byte_summary("canonical_arguments", procedure.canonical_arguments()),
            |runtime| debug_invocation_fields(runtime.invocation()),
        ),
    };

    append_id_roles(&mut arguments, procedure);
    if !procedure.canonical_payload().is_empty() {
        append_field_separator(&mut arguments);
        arguments.push_str(&byte_summary(
            "canonical_payload",
            procedure.canonical_payload(),
        ));
    }
    Ok((name, arguments))
}

fn debug_invocation_fields(invocation: &CanonicalInvocation) -> String {
    let debug = format!("{invocation:?}");
    if debug.chars().count() > MAX_INLINE_DEBUG_CHARS {
        return format!(
            "typed_arguments_chars={}, summary=omitted",
            debug.chars().count()
        );
    }
    match (debug.find('{'), debug.rfind('}')) {
        (Some(open), Some(close)) if open < close => debug[open + 1..close].trim().to_owned(),
        _ => String::new(),
    }
}

fn append_id_roles(arguments: &mut String, procedure: &CanonicalProcedure) {
    if !procedure.input_ids().is_empty() {
        append_field_separator(arguments);
        let _ = write!(arguments, "input_ids={:?}", procedure.input_ids());
    }
    if !procedure.output_ids().is_empty() {
        append_field_separator(arguments);
        let _ = write!(arguments, "output_ids={:?}", procedure.output_ids());
    }
    if !procedure.asset_ids().is_empty() {
        append_field_separator(arguments);
        let ids = procedure
            .asset_ids()
            .iter()
            .map(|id| {
                blake3::Hash::from_bytes(*id.as_bytes())
                    .to_hex()
                    .to_string()
            })
            .collect::<Vec<_>>();
        let _ = write!(arguments, "asset_ids={ids:?}");
    }
}

fn append_field_separator(arguments: &mut String) {
    if !arguments.is_empty() {
        arguments.push_str(", ");
    }
}

fn byte_summary(field: &str, bytes: &[u8]) -> String {
    let digest = blake3::hash(bytes);
    format!(
        "{field}_bytes={}, {field}_blake3={}",
        bytes.len(),
        digest.to_hex()
    )
}
