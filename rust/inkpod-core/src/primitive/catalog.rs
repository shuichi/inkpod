//! Closed primitive catalog and build-time replay contract.

use super::*;
use std::sync::LazyLock;

const CATALOG_CONTEXT: &str = "org.inkpod.primitive-catalog.v1";
const ARGUMENT_SCHEMA_CONTEXT: &str = "org.inkpod.primitive-argument-schema.v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PrimitiveCatalogEntry {
    pub(crate) id: PrimitiveId,
    pub(crate) schema_version: u16,
    pub(crate) canonical_name: &'static str,
    pub(crate) argument_schema: &'static str,
    pub(crate) semantics_revision: u32,
    pub(crate) work_formula_id: u32,
    pub(crate) replayable: bool,
}

macro_rules! entry {
    ($id:ident, $schema:expr, $name:literal, $semantics:expr, $work:expr) => {
        PrimitiveCatalogEntry {
            id: PrimitiveId::$id,
            schema_version: $schema,
            canonical_name: $name,
            argument_schema: concat!($name, "/canonical-v", stringify!($schema)),
            semantics_revision: $semantics,
            work_formula_id: $work,
            replayable: true,
        }
    };
    ($id:ident, $schema:expr, $name:literal, $semantics:expr, $work:expr, session) => {
        PrimitiveCatalogEntry {
            id: PrimitiveId::$id,
            schema_version: $schema,
            canonical_name: $name,
            argument_schema: concat!($name, "/canonical-v", stringify!($schema)),
            semantics_revision: $semantics,
            work_formula_id: $work,
            replayable: false,
        }
    };
}

// Sorted by stable PrimitiveId. Schema 2 is the canonical typed-invocation layout:
// every document coordinate/diameter/scale is fixed point, pressure is u16,
// and angles are u32 turns. The four original kernel records retain their
// independently versioned layouts.
const PRIMITIVE_CATALOG: &[PrimitiveCatalogEntry] = &[
    entry!(UPDATE_PAPER_FRAMES, 2, "UpdatePaperFrames", 2, 0x0001_0001),
    entry!(CREATE_LAYER, 2, "CreateLayer", 2, 0x0002_0001),
    entry!(DUPLICATE_LAYER, 2, "DuplicateLayer", 2, 0x0002_0002),
    entry!(DELETE_LAYER, 2, "DeleteLayer", 2, 0x0002_0003),
    entry!(REORDER_LAYER, 2, "ReorderLayer", 2, 0x0002_0004),
    entry!(
        SET_LAYER_PROPERTIES,
        2,
        "SetLayerProperties",
        2,
        0x0002_0005
    ),
    entry!(CREATE_PLANE, 2, "CreatePlane", 2, 0x0002_0011),
    entry!(DUPLICATE_PLANE, 2, "DuplicatePlane", 2, 0x0002_0012),
    entry!(DELETE_PLANE, 2, "DeletePlane", 2, 0x0002_0013),
    entry!(REORDER_PLANE, 2, "ReorderPlane", 2, 0x0002_0014),
    entry!(
        SET_PLANE_PROPERTIES,
        2,
        "SetPlaneProperties",
        2,
        0x0002_0015
    ),
    entry!(CONVERT_PLANE, 2, "ConvertPlane", 2, 0x0002_0016),
    entry!(MERGE_PLANE, 2, "MergePlane", 2, 0x0002_0017),
    entry!(CONVERT_LAYER, 2, "ConvertLayer", 2, 0x0002_0021),
    entry!(MERGE_LAYER, 2, "MergeLayer", 2, 0x0002_0022),
    entry!(
        DELETE_HIDDEN_LAYERS,
        2,
        "DeleteHiddenLayers",
        2,
        0x0002_0023
    ),
    entry!(EDIT_TARGETS, 2, "EditTargets", 1, 0x0002_0030),
    entry!(EDIT_ANNOTATIONS, 2, "EditAnnotations", 1, 0x0002_0040),
    entry!(EDIT_SHOOTING_FRAME, 2, "EditShootingFrame", 1, 0x0002_0050),
    entry!(SET_MAIN_LINE_COLOR, 1, "SetMainLineColor", 3, 1),
    entry!(REPLACE_PALETTE, 1, "ReplacePalette", 3, 2),
    entry!(REPLACE_COLOR_CHART, 1, "ReplaceColorChart", 1, 5),
    entry!(ADD_GUIDE, 2, "AddGuide", 2, 0x0004_0001),
    entry!(MOVE_GUIDE, 2, "MoveGuide", 2, 0x0004_0002),
    entry!(DELETE_GUIDE, 2, "DeleteGuide", 2, 0x0004_0003),
    entry!(SET_GRID, 2, "SetGrid", 2, 0x0004_0010),
    entry!(DELETE_ALL_GUIDES, 2, "DeleteAllGuides", 2, 0x0004_0011),
    entry!(APPLY_RASTER_STROKE, 3, "ApplyRasterStroke", 5, 3),
    entry!(APPLY_FILL, 2, "ApplyFill", 2, 0x0005_0002),
    entry!(APPLY_GEOMETRY, 2, "ApplyGeometry", 1, 0x0005_0003),
    entry!(APPLY_GRADIENT, 2, "ApplyGradient", 2, 0x0005_0010),
    entry!(
        APPLY_BOUNDARY_AIRBRUSH,
        2,
        "ApplyBoundaryAirbrush",
        2,
        0x0005_0011
    ),
    entry!(APPLY_BLUR, 2, "ApplyBlur", 2, 0x0005_0012),
    entry!(APPLY_AIRBRUSH, 2, "ApplyAirbrush", 2, 0x0005_0013),
    entry!(
        APPLY_AIRBRUSH_GESTURE,
        2,
        "ApplyAirbrushGesture",
        2,
        0x0005_0014
    ),
    entry!(APPLY_STAMP, 2, "ApplyStamp", 2, 0x0005_0015),
    entry!(APPLY_STAMP_GESTURE, 2, "ApplyStampGesture", 2, 0x0005_0016),
    entry!(APPLY_BLUR_TOOL, 2, "ApplyBlurTool", 2, 0x0005_0017),
    entry!(APPLY_DUST_REMOVAL, 2, "ApplyDustRemoval", 2, 0x0005_0018),
    entry!(EDIT_PLANE_ALPHA, 2, "EditPlaneAlpha", 2, 0x0005_0019),
    entry!(
        APPLY_ALPHA_GRADIENT,
        2,
        "ApplyAlphaGradient",
        2,
        0x0005_001a
    ),
    entry!(APPLY_FILTER, 2, "ApplyFilter", 2, 0x0005_0020),
    entry!(
        CREATE_ADJUSTMENT_LAYER,
        2,
        "CreateAdjustmentLayer",
        2,
        0x0005_0030
    ),
    entry!(
        UPDATE_ADJUSTMENT_LAYER,
        2,
        "UpdateAdjustmentLayer",
        2,
        0x0005_0031
    ),
    entry!(
        REPLACE_RASTER_COLORS,
        2,
        "ReplaceRasterColors",
        2,
        0x0005_0040
    ),
    entry!(
        SEPARATE_RASTER_COLORS,
        2,
        "SeparateRasterColors",
        3,
        0x0005_0041
    ),
    entry!(
        RESTORE_SELECTED_PIXELS,
        2,
        "RestoreSelectedPixels",
        2,
        0x0005_0042
    ),
    entry!(
        SCOPED_COLOR_REPLACE,
        2,
        "ScopedColorReplace",
        1,
        0x0005_0043
    ),
    entry!(APPLY_SELECTION, 2, "ApplySelection", 3, 0x0006_0001),
    entry!(INVERT_SELECTION, 2, "InvertSelection", 2, 0x0006_0002),
    entry!(CLEAR_SELECTION, 2, "ClearSelection", 2, 0x0006_0003),
    entry!(RESIZE_SELECTION, 2, "ResizeSelection", 2, 0x0006_0004),
    entry!(SELECT_COLOR, 2, "SelectColor", 2, 0x0006_0005),
    entry!(
        SELECT_OUTPUT_COLOR_GUARD,
        2,
        "SelectOutputColorGuard",
        1,
        0x0006_0006
    ),
    entry!(SELECTION_TO_LAYER, 2, "SelectionToLayer", 2, 0x0006_0010),
    entry!(
        SELECTION_FROM_LAYER,
        2,
        "SelectionFromLayer",
        2,
        0x0006_0011
    ),
    entry!(
        CLEAR_SELECTED_CONTENT,
        2,
        "ClearSelectedContent",
        2,
        0x0006_0020
    ),
    entry!(COMMIT_FLOATING, 3, "CommitFloating", 3, 0x0006_0021),
    entry!(MIRROR_DOCUMENT, 2, "MirrorDocument", 2, 0x0007_0001),
    entry!(ROTATE_DOCUMENT, 2, "RotateDocument", 2, 0x0007_0002),
    entry!(RESIZE_DOCUMENT, 2, "ResizeDocument", 2, 0x0007_0003),
    entry!(VECTOR_ADD_PATH, 2, "VectorAddPath", 2, 0x0008_0001),
    entry!(VECTOR_ADD_FILL, 2, "VectorAddFill", 2, 0x0008_0002),
    entry!(VECTOR_ERASE, 2, "VectorErase", 2, 0x0008_0003),
    entry!(VECTOR_CONNECT, 2, "VectorConnect", 2, 0x0008_0004),
    entry!(
        VECTOR_CORRECT_WIDTH,
        2,
        "VectorCorrectWidth",
        2,
        0x0008_0005
    ),
    entry!(
        RASTERIZE_VECTOR_LAYER,
        2,
        "RasterizeVectorLayer",
        2,
        0x0008_0010
    ),
    entry!(
        VECTORIZE_RASTER_PLANE,
        2,
        "VectorizeRasterPlane",
        2,
        0x0008_0011
    ),
    entry!(
        VECTORIZE_RASTER_PLANE_INTO_NEW_LAYER,
        2,
        "VectorizeRasterPlaneIntoNewLayer",
        2,
        0x0008_0012
    ),
    entry!(IMPORT_RASTER_ASSET, 1, "ImportRasterAsset", 1, 4),
    entry!(
        LIGHT_TABLE_SET_GLOBAL_OPACITY,
        2,
        "LightTableSetGlobalOpacity",
        2,
        0x000a_0001
    ),
    entry!(
        LIGHT_TABLE_CREATE_SET,
        2,
        "LightTableCreateSet",
        2,
        0x000a_0002
    ),
    entry!(
        LIGHT_TABLE_DUPLICATE_SET,
        2,
        "LightTableDuplicateSet",
        2,
        0x000a_0003
    ),
    entry!(
        LIGHT_TABLE_DELETE_SET,
        2,
        "LightTableDeleteSet",
        2,
        0x000a_0004
    ),
    entry!(
        LIGHT_TABLE_RENAME_SET,
        2,
        "LightTableRenameSet",
        2,
        0x000a_0005
    ),
    entry!(
        LIGHT_TABLE_REORDER_SET,
        2,
        "LightTableReorderSet",
        2,
        0x000a_0006
    ),
    entry!(
        LIGHT_TABLE_SET_ACTIVE,
        2,
        "LightTableSetActive",
        2,
        0x000a_0007
    ),
    entry!(LIGHT_TABLE_ADD_ITEM, 2, "LightTableAddItem", 2, 0x000a_0010),
    entry!(
        LIGHT_TABLE_UPDATE_ITEM_PROPERTIES,
        2,
        "LightTableUpdateItemProperties",
        2,
        0x000a_0011
    ),
    entry!(
        LIGHT_TABLE_UPDATE_ITEM,
        2,
        "LightTableUpdateItem",
        2,
        0x000a_0012
    ),
    entry!(
        LIGHT_TABLE_REMOVE_ITEM,
        2,
        "LightTableRemoveItem",
        2,
        0x000a_0013
    ),
    entry!(
        LIGHT_TABLE_REORDER_ITEM,
        2,
        "LightTableReorderItem",
        2,
        0x000a_0014
    ),
    entry!(
        LIGHT_TABLE_SWAP_WITH_ACTIVE,
        1,
        "LightTableSwapWithActive",
        2,
        0x000a_0015,
        session
    ),
    entry!(
        LIGHT_TABLE_BULK_REGISTER,
        2,
        "LightTableBulkRegister",
        1,
        0x000a_0016
    ),
];

static REPLAY_CONTRACT: LazyLock<ReplayContract> = LazyLock::new(|| {
    let mut hasher = blake3::Hasher::new_derive_key(CATALOG_CONTEXT);
    hasher.update(&(PRIMITIVE_CATALOG.len() as u32).to_le_bytes());
    let mut previous = 0_u32;
    for entry in PRIMITIVE_CATALOG {
        assert!(
            entry.id.get() > previous,
            "primitive catalog must be sorted"
        );
        previous = entry.id.get();
        let name = entry.canonical_name.as_bytes();
        let argument_schema_digest =
            blake3::derive_key(ARGUMENT_SCHEMA_CONTEXT, entry.argument_schema.as_bytes());
        hasher.update(&entry.id.get().to_le_bytes());
        hasher.update(&entry.schema_version.to_le_bytes());
        hasher.update(&(name.len() as u16).to_le_bytes());
        hasher.update(name);
        hasher.update(&argument_schema_digest);
        hasher.update(&entry.semantics_revision.to_le_bytes());
        hasher.update(&entry.work_formula_id.to_le_bytes());
        hasher.update(&[u8::from(entry.replayable)]);
    }
    ReplayContract {
        replay_epoch: ReplayEpoch::CURRENT,
        procedure_format_version: PROCEDURE_FORMAT_VERSION,
        canonical_numeric_version: CANONICAL_NUMERIC_VERSION,
        primitive_count: PRIMITIVE_CATALOG.len() as u32,
        primitive_catalog_digest: *hasher.finalize().as_bytes(),
    }
});

/// Returns the immutable replay/build contract for this Core version.
#[must_use]
pub fn replay_contract() -> ReplayContract {
    *REPLAY_CONTRACT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_sorted_unique_bounded_and_matches_replay_schema_lookup() {
        assert_eq!(PRIMITIVE_CATALOG.len(), 84);
        for pair in PRIMITIVE_CATALOG.windows(2) {
            assert!(pair[0].id < pair[1].id);
        }
        for entry in PRIMITIVE_CATALOG {
            assert_ne!(entry.id.get(), 0);
            assert!(entry.id.get() < 0x8000_0000);
            assert_ne!(entry.schema_version, 0);
            assert_ne!(entry.semantics_revision, 0);
            assert_ne!(entry.work_formula_id, 0);
            if entry.replayable {
                assert_eq!(
                    super::super::executor::current_primitive_schema_version(entry.id),
                    Some(entry.schema_version),
                    "{}",
                    entry.canonical_name
                );
            }
        }
    }

    #[test]
    fn replay_contract_is_closed_and_nonzero() {
        let contract = replay_contract();
        assert_eq!(contract.replay_epoch(), ReplayEpoch::CURRENT);
        assert_eq!(
            contract.procedure_format_version(),
            PROCEDURE_FORMAT_VERSION
        );
        assert_eq!(contract.canonical_numeric_version(), 1);
        assert_eq!(contract.primitive_count() as usize, PRIMITIVE_CATALOG.len());
        assert_ne!(contract.primitive_catalog_digest(), &[0; 32]);
    }
}
