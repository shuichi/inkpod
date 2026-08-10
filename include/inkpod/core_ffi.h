#ifndef INKPOD_CORE_FFI_H
#define INKPOD_CORE_FFI_H

/**
 * @file core_ffi.h
 * @brief Inkpod Rust Core の versioned C ABI 仕様。
 *
 * このヘッダーを公開 API の正本とする。`docs/ffi.md` は本契約の利用ガイドであり、
 * 宣言や所有権規則が食い違う場合は本ファイルを優先する。
 *
 * @par 共通の構造体規則
 * 拡張可能な入出力構造体は先頭が `uint32_t struct_size` である。呼び出し側は
 * `struct_size = sizeof(その構造体)` を設定する。Core は ABI v8 で既知の末尾まで
 * 読み書きできるサイズ、アラインメント、stride、count と全バイト範囲を検証してから
 * ポインターを参照する。構造体ポインターは個別に NULL 可と明記したものを除き非 NULL。
 * count が 0 の任意 span だけはデータポインターを NULL にできる。入力構造体、出力構造体、
 * opaque object の記憶域を互いに重ねてはならない。
 *
 * @par 共通の所有権規則
 * `const T*` の入力と入力 span は、個別に明記しない限りその呼び出し中だけの
 * borrowed（借用）であり、戻る前に必要な意味値がコピーされる。`T** out_*` で生成する
 * opaque handle は Rust-owned（Rust 所有）で、対応する `*_release` または
 * `inkpod_core_destroy` が所有権を消費して owner 変数を NULL にする。生成先は呼び出し前に
 * NULL でなければならない。snapshot の view/span と batch/sequence の文字列・byte span は、
 * 親 handle の解放まで borrowed である。解放後の別名ポインターはすべて無効になる。
 *
 * @par Core のスレッド規則
 * `InkpodCore` は作成スレッドに固定された single-writer である。`inkpod_core_*`、Core からの
 * snapshot 構築、Core の destroy は作成スレッドから呼ぶ。違反は
 * `INKPOD_STATUS_WRONG_THREAD` で、handle や出力を消費しない。immutable snapshot の参照と
 * release、task の create/query/cancel/release、および Core を取らない immutable handle の
 * accessor/release は任意スレッドでよいが、同じ owner の release と参照を呼び出し側で
 * 同期する。
 *
 * @par revision、dirty、Undo と失敗時の原子性
 * 文書編集 API は、成功して実変更があると document revision を 1 回進め、document dirty にし、
 * 原則 1 Undo 単位を追加する。EditorState の意味変更は独立した EditorRevision/digest/editor dirty
 * だけを進める。公開 session dirty は `document_dirty || editor_dirty` である。query、view、shortcut、
 * snapshot、task はいずれの意味状態も変えない。current native formatの通常保存はrevision/Undoを変えず
 * document/editor両savepointを現在位置へ移し、session dirtyを解消する。
 * autosaveは両方の通常savepointを変えない。個別に部分出力を明記した
 * `BUFFER_TOO_SMALL`、`FILL_OVERFLOW`、cancelled batch report、error-message API を除き、失敗時は
 * 文書、履歴、所有権出力を変更せず、通常の値出力は未使用とする。
 *
 * @par stroke／preview の排他状態
 * 1 Core に live stroke、filter/dust preview、floating paste はそれぞれ高々 1 個である。
 * stroke と filter/dust preview は同時に存在できない。競合する文書編集、履歴、保存、open、
 * tree/plane 操作、別 preview の開始は `INKPOD_STATUS_INVALID_STATE` となる。stroke の
 * begin/append と preview の begin/update は committed revision、dirty、Undo を変えず、
 * snapshot だけが transient preview を観測できる。end/apply は高々 1 Undo 単位、
 * cancel は committed base を完全に保つ。
 *
 * @par 共通ステータス
 * 主な戻り値は `INKPOD_STATUS_OK`、NULL・サイズ・enum・flags・範囲不正の
 * `INKPOD_STATUS_INVALID_ARGUMENT`、状態競合の `INKPOD_STATUS_INVALID_STATE`、文書未作成の
 * `INKPOD_STATUS_NO_DOCUMENT`、スレッド違反の `INKPOD_STATUS_WRONG_THREAD` である。
 * ファイル API は `INKPOD_STATUS_IO_ERROR`、task 対応 API は `INKPOD_STATUS_CANCELLED`、
 * caller buffer API は `INKPOD_STATUS_BUFFER_TOO_SMALL` も返す。Rust panic は ABI を越えず
 * `INKPOD_STATUS_PANIC` へ変換される。
 */

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define INKPOD_ABI_VERSION UINT32_C(9)
#define INKPOD_FEATURE_NONE UINT64_C(0)

/** @brief すべての fallible API が返す固定幅ステータス型。 */
typedef uint32_t InkpodStatus;
#define INKPOD_STATUS_OK UINT32_C(0)
#define INKPOD_STATUS_INVALID_ARGUMENT UINT32_C(1)
#define INKPOD_STATUS_INCOMPATIBLE_ABI UINT32_C(2)
#define INKPOD_STATUS_BUFFER_TOO_SMALL UINT32_C(3)
#define INKPOD_STATUS_UNSUPPORTED UINT32_C(4)
#define INKPOD_STATUS_PANIC UINT32_C(5)
#define INKPOD_STATUS_WRONG_THREAD UINT32_C(6)
#define INKPOD_STATUS_IO_ERROR UINT32_C(7)
#define INKPOD_STATUS_INVALID_STATE UINT32_C(8)
#define INKPOD_STATUS_NO_DOCUMENT UINT32_C(9)
#define INKPOD_STATUS_CANCELLED UINT32_C(10)
#define INKPOD_STATUS_FILL_OVERFLOW UINT32_C(11)
#define INKPOD_STATUS_UNSAVED_CHANGES UINT32_C(12)

/** @brief ABI-v3 Rust-owned object の閉じた type namespace。 */
typedef uint32_t InkpodObjectType;
#define INKPOD_OBJECT_NONE UINT32_C(0)
#define INKPOD_OBJECT_CORE UINT32_C(1)
#define INKPOD_OBJECT_SNAPSHOT UINT32_C(2)
#define INKPOD_OBJECT_TASK UINT32_C(3)
#define INKPOD_OBJECT_ASSET UINT32_C(4)
#define INKPOD_OBJECT_SAMPLE_STREAM UINT32_C(5)
#define INKPOD_OBJECT_COLOR_ARRAY UINT32_C(6)
#define INKPOD_OBJECT_THUMBNAIL UINT32_C(7)
#define INKPOD_OBJECT_EXPORT UINT32_C(8)

/** @brief canonical primitive catalog の stable opcode。 */
typedef uint32_t InkpodPrimitiveOpcode;
#define INKPOD_PRIMITIVE_SET_MAIN_LINE_COLOR UINT32_C(0x00030001)
#define INKPOD_PRIMITIVE_REPLACE_PALETTE UINT32_C(0x00030002)
#define INKPOD_PRIMITIVE_APPLY_RASTER_STROKE UINT32_C(0x00050001)
#define INKPOD_PRIMITIVE_IMPORT_RASTER_ASSET UINT32_C(0x00090001)
#define INKPOD_PRIMITIVE_RESULT_COMMITTED (UINT32_C(1) << 0)

#define INKPOD_PERSISTENCE_CHECKPOINT_DUE (UINT32_C(1) << 0)
#define INKPOD_NATIVE_OPEN_NOT_OPENED UINT32_C(0)
#define INKPOD_NATIVE_OPEN_FULL_REPLAY UINT32_C(1)
#define INKPOD_NATIVE_OPEN_CHECKPOINT UINT32_C(2)

/** @brief snapshot が公開する表示用 pixel format 型。 */
typedef uint32_t InkpodPixelFormat;
#define INKPOD_PIXEL_FORMAT_INVALID UINT32_C(0)
#define INKPOD_PIXEL_FORMAT_PREMULTIPLIED_BGRA8 UINT32_C(1)
#define INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE (UINT64_C(1) << 0)
#define INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA (UINT64_C(1) << 1)
#define INKPOD_SNAPSHOT_FEATURE_SOLID_WHITE_BASE (UINT64_C(1) << 2)

/** @brief PNG/TIFF/TGA/BMP import/export の format 識別子型。 */
typedef uint32_t InkpodCommonRasterFormat;
#define INKPOD_COMMON_RASTER_PNG UINT32_C(1)
#define INKPOD_COMMON_RASTER_TIFF UINT32_C(2)
#define INKPOD_COMMON_RASTER_TGA UINT32_C(3)
#define INKPOD_COMMON_RASTER_BMP UINT32_C(4)

/** @brief 互換用の基本 plane 識別子型。 */
typedef uint32_t InkpodPlaneKind;
#define INKPOD_PLANE_MAIN_LINE UINT32_C(1)
#define INKPOD_PLANE_COLOR UINT32_C(2)

/** @brief raster stroke tool の識別子型。 */
typedef uint32_t InkpodPaintTool;
#define INKPOD_TOOL_PENCIL UINT32_C(1)
#define INKPOD_TOOL_BRUSH UINT32_C(2)
#define INKPOD_TOOL_ERASER UINT32_C(3)

/** @brief Core-owned EditorState が保持する全 editor tool の安定識別子型。 */
typedef uint32_t InkpodEditorTool;
#define INKPOD_EDITOR_TOOL_PENCIL UINT32_C(1)
#define INKPOD_EDITOR_TOOL_BRUSH UINT32_C(2)
#define INKPOD_EDITOR_TOOL_ERASER UINT32_C(3)
#define INKPOD_EDITOR_TOOL_FILL UINT32_C(1001)
#define INKPOD_EDITOR_TOOL_EYEDROPPER UINT32_C(1002)
#define INKPOD_EDITOR_TOOL_BOX_ZOOM UINT32_C(1003)
#define INKPOD_EDITOR_TOOL_GUIDE_MOVE UINT32_C(1004)
#define INKPOD_EDITOR_TOOL_SELECTION UINT32_C(1005)
#define INKPOD_EDITOR_TOOL_COLOR_REPLACE UINT32_C(1008)
#define INKPOD_EDITOR_TOOL_FLOATING_TRANSFORM UINT32_C(1006)
#define INKPOD_EDITOR_TOOL_LIGHT_TABLE_MOVE UINT32_C(1007)
#define INKPOD_EDITOR_TOOL_EFFECT_GRADIENT UINT32_C(1101)
#define INKPOD_EDITOR_TOOL_EFFECT_AIRBRUSH UINT32_C(1102)
#define INKPOD_EDITOR_TOOL_EFFECT_BLUR UINT32_C(1103)
#define INKPOD_EDITOR_TOOL_EFFECT_STAMP UINT32_C(1104)
#define INKPOD_EDITOR_TOOL_EFFECT_DUST UINT32_C(1105)
#define INKPOD_EDITOR_TOOL_EFFECT_ALPHA_GRADIENT UINT32_C(1106)
#define INKPOD_EDITOR_TOOL_VECTOR_LINE UINT32_C(1201)
#define INKPOD_EDITOR_TOOL_VECTOR_CURVE UINT32_C(1202)
#define INKPOD_EDITOR_TOOL_VECTOR_RECTANGLE UINT32_C(1203)
#define INKPOD_EDITOR_TOOL_VECTOR_ELLIPSE UINT32_C(1204)
#define INKPOD_EDITOR_TOOL_VECTOR_POLYLINE UINT32_C(1205)
#define INKPOD_EDITOR_TOOL_VECTOR_ERASER UINT32_C(1206)

#define INKPOD_EDITOR_STATE_DIRTY (UINT32_C(1) << 0)
#define INKPOD_EDITOR_STATE_HAS_LAST_COLOR_TOOL (UINT32_C(1) << 1)
#define INKPOD_EDITOR_STATE_HAS_TARGET (UINT32_C(1) << 2)
#define INKPOD_EDITOR_STATE_HAS_PALETTE_CURSOR (UINT32_C(1) << 3)
#define INKPOD_EDITOR_STATE_HAS_CURRENT_COLOR (UINT32_C(1) << 4)

#define INKPOD_EDITOR_FILL_OVERFLOW_ABORT (UINT64_C(1) << 0)
#define INKPOD_EDITOR_FILL_DETACHED_REGIONS (UINT64_C(1) << 1)
#define INKPOD_EDITOR_FILL_TRANSPARENT_ONLY (UINT64_C(1) << 2)
#define INKPOD_EDITOR_FILL_DOCUMENT_SELECTION (UINT64_C(1) << 3)
#define INKPOD_EDITOR_FILL_LIGHT_TABLE_BOUNDARY (UINT64_C(1) << 4)
#define INKPOD_EDITOR_FILL_LIGHT_TABLE_COLOR (UINT64_C(1) << 5)
#define INKPOD_EDITOR_FILL_FLAGS ((UINT64_C(1) << 6) - UINT64_C(1))

typedef uint32_t InkpodEditorUpdateKind;
#define INKPOD_EDITOR_UPDATE_ACTIVE_TOOL UINT32_C(1)
#define INKPOD_EDITOR_UPDATE_TOOL_COLOR UINT32_C(2)
#define INKPOD_EDITOR_UPDATE_TOOL_DIAMETER UINT32_C(3)
#define INKPOD_EDITOR_UPDATE_FILL_OPTIONS UINT32_C(4)
#define INKPOD_EDITOR_UPDATE_SELECTION_OPTIONS UINT32_C(5)
#define INKPOD_EDITOR_UPDATE_VECTOR_OPTIONS UINT32_C(6)
#define INKPOD_EDITOR_UPDATE_ACTIVE_TARGET UINT32_C(7)
#define INKPOD_EDITOR_UPDATE_PALETTE_CURSOR UINT32_C(8)
#define INKPOD_EDITOR_UPDATE_BRUSH_OPTIONS UINT32_C(9)
#define INKPOD_EDITOR_UPDATE_PALETTE_CURSOR_PRESENT (UINT64_C(1) << 0)

/** @brief brush dab footprint。 */
typedef uint32_t InkpodBrushShape;
#define INKPOD_BRUSH_ROUND UINT32_C(1)
#define INKPOD_BRUSH_SQUARE UINT32_C(2)

/** @brief immutable pre-stroke pixel predicate。 */
typedef uint32_t InkpodStartColorPredicate;
#define INKPOD_START_COLOR_ANY UINT32_C(0)
#define INKPOD_START_COLOR_EXACT_NATIVE UINT32_C(1)

#define INKPOD_EDITOR_MAX_INCLUSION_COLORS UINT32_C(6)

/** @brief 入力座標が document logical か client device pixel かを表す型。 */
typedef uint32_t InkpodCoordinateSpace;
#define INKPOD_COORDINATE_SPACE_DOCUMENT UINT32_C(1)
#define INKPOD_COORDINATE_SPACE_DEVICE UINT32_C(2)

#define INKPOD_STROKE_FLAG_AUTO_ERASE (UINT64_C(1) << 0)
#define INKPOD_STROKE_FLAG_PRESSURE_SIZE (UINT64_C(1) << 1)

#define INKPOD_DOCUMENT_FLAG_DIRTY (UINT32_C(1) << 0)
#define INKPOD_DOCUMENT_FLAG_CAN_UNDO (UINT32_C(1) << 1)
#define INKPOD_DOCUMENT_FLAG_CAN_REDO (UINT32_C(1) << 2)
#define INKPOD_DOCUMENT_FLAG_RECOVERED (UINT32_C(1) << 3)
#define INKPOD_HISTORY_ITEM_APPLIED (UINT32_C(1) << 0)

/** @brief pan/zoom/flip/overlay など view-only command の識別子型。 */
typedef uint32_t InkpodViewCommandKind;
#define INKPOD_VIEW_PAN_BY UINT32_C(1)
#define INKPOD_VIEW_ZOOM_AT UINT32_C(2)
#define INKPOD_VIEW_FIT UINT32_C(3)
#define INKPOD_VIEW_ONE_TO_ONE UINT32_C(4)
#define INKPOD_VIEW_VIEWPORT_RESIZED UINT32_C(5)
#define INKPOD_VIEW_BOX_ZOOM UINT32_C(6)
#define INKPOD_VIEW_FLIP_HORIZONTAL UINT32_C(7)
#define INKPOD_VIEW_FLIP_VERTICAL UINT32_C(8)
#define INKPOD_VIEW_SET_RULER_VISIBLE UINT32_C(9)
#define INKPOD_VIEW_SET_GUIDES_VISIBLE UINT32_C(10)
#define INKPOD_VIEW_SET_GRID_VISIBLE UINT32_C(11)
#define INKPOD_VIEW_SET_SNAP_ENABLED UINT32_C(12)
#define INKPOD_VIEW_SET_TRANSPARENT_VISIBLE UINT32_C(13)
#define INKPOD_VIEW_SET_ALPHA_VISIBLE UINT32_C(14)
#define INKPOD_VIEW_SET_GUIDE_SNAP_ENABLED UINT32_C(15)
#define INKPOD_VIEW_SET_GRID_SNAP_ENABLED UINT32_C(16)
#define INKPOD_VIEW_SET_VECTOR_ANTIALIAS UINT32_C(17)
#define INKPOD_VIEW_SET_VECTOR_CENTERLINE_MODE UINT32_C(18)
#define INKPOD_VIEW_SET_VECTOR_ENDPOINTS_VISIBLE UINT32_C(19)

typedef uint32_t InkpodVectorCenterlineMode;
#define INKPOD_VECTOR_CENTERLINE_HIDDEN UINT32_C(0)
#define INKPOD_VECTOR_CENTERLINE_OVERLAY UINT32_C(1)
#define INKPOD_VECTOR_CENTERLINE_ONLY UINT32_C(2)

#define INKPOD_VECTOR_DIAGNOSTIC_ANTIALIAS (UINT32_C(1) << 0)
#define INKPOD_VECTOR_DIAGNOSTIC_CENTERLINE_VISIBLE (UINT32_C(1) << 1)
#define INKPOD_VECTOR_DIAGNOSTIC_CENTERLINE_ONLY (UINT32_C(1) << 2)
#define INKPOD_VECTOR_DIAGNOSTIC_ENDPOINTS_VISIBLE (UINT32_C(1) << 3)

typedef uint32_t InkpodVectorEndpointKind;
#define INKPOD_VECTOR_ENDPOINT_START UINT32_C(1)
#define INKPOD_VECTOR_ENDPOINT_END UINT32_C(2)

#define INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL (UINT32_C(1) << 0)
#define INKPOD_SNAPSHOT_TRANSFORM_FLIP_VERTICAL (UINT32_C(1) << 1)
#define INKPOD_SNAPSHOT_OVERLAY_RULER_VISIBLE (UINT32_C(1) << 0)
#define INKPOD_SNAPSHOT_OVERLAY_GUIDES_VISIBLE (UINT32_C(1) << 1)
#define INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE (UINT32_C(1) << 2)
#define INKPOD_SNAPSHOT_OVERLAY_SNAP_ENABLED (UINT32_C(1) << 3)
#define INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW (UINT32_C(1) << 4)
#define INKPOD_SNAPSHOT_OVERLAY_ALPHA_VIEW (UINT32_C(1) << 5)

#define INKPOD_SHORTCUT_MODIFIER_CONTROL (UINT32_C(1) << 0)
#define INKPOD_SHORTCUT_MODIFIER_SHIFT (UINT32_C(1) << 1)
#define INKPOD_SHORTCUT_MODIFIER_ALT (UINT32_C(1) << 2)
#define INKPOD_SHORTCUT_MODIFIER_EXTENDED (UINT32_C(1) << 3)
#define INKPOD_SHORTCUT_MAX_STROKES UINT32_C(4)
typedef uint32_t InkpodShortcutMatch;
#define INKPOD_SHORTCUT_MATCH_NONE UINT32_C(0)
#define INKPOD_SHORTCUT_MATCH_PREFIX UINT32_C(1)
#define INKPOD_SHORTCUT_MATCH_EXACT UINT32_C(2)

/** @brief binary、grayscale、RGBA 8/16 bit を明示する色深度型。 */
typedef uint32_t InkpodColorDepth;
#define INKPOD_COLOR_DEPTH_BINARY UINT32_C(1)
#define INKPOD_COLOR_DEPTH_GRAYSCALE_8 UINT32_C(2)
#define INKPOD_COLOR_DEPTH_GRAYSCALE_16 UINT32_C(3)
#define INKPOD_COLOR_DEPTH_8 UINT32_C(8)
#define INKPOD_COLOR_DEPTH_16 UINT32_C(16)

/** @brief seed、closed-region、extension fill の識別子型。 */
typedef uint32_t InkpodFillOperation;
#define INKPOD_FILL_SEED UINT32_C(1)
#define INKPOD_FILL_CLOSED_REGION UINT32_C(2)
#define INKPOD_FILL_EXTENSION UINT32_C(3)
#define INKPOD_FILL_FLAG_DETACHED_REGIONS (UINT64_C(1) << 0)
#define INKPOD_FILL_FLAG_OVERFLOW_ABORT (UINT64_C(1) << 1)
#define INKPOD_FILL_FLAG_TRANSPARENT_ONLY (UINT64_C(1) << 2)
#define INKPOD_FILL_FLAG_SELECTION_PRESENT (UINT64_C(1) << 3)
#define INKPOD_FILL_FLAG_LIGHT_TABLE_BOUNDARY (UINT64_C(1) << 4)
#define INKPOD_FILL_FLAG_LIGHT_TABLE_COLOR (UINT64_C(1) << 5)
#define INKPOD_FILL_FLAG_DOCUMENT_SELECTION (UINT64_C(1) << 6)

/** @brief fill の包含色条件を表す型。 */
typedef uint32_t InkpodInclusionMode;
#define INKPOD_INCLUSION_NONE UINT32_C(0)
#define INKPOD_INCLUSION_SPECIFIED UINT32_C(1)
#define INKPOD_INCLUSION_EXCEPT_SPECIFIED UINT32_C(2)

#define INKPOD_FILL_RESULT_FLAG_LEAK_CANDIDATE (UINT32_C(1) << 0)

/** @brief eyedropper の読み取り元を表す型。 */
typedef uint32_t InkpodEyedropperSource;
#define INKPOD_EYEDROPPER_TOPMOST_NONTRANSPARENT UINT32_C(1)
#define INKPOD_EYEDROPPER_SELECTED_PLANE UINT32_C(2)
#define INKPOD_EYEDROPPER_COMPOSITE UINT32_C(3)
#define INKPOD_EYEDROPPER_LIGHT_TABLE_TOPMOST UINT32_C(4)

/** @brief 一時的な色チェック表示モード型。 */
typedef uint32_t InkpodColorCheckMode;
#define INKPOD_COLOR_CHECK_OFF UINT32_C(0)
#define INKPOD_COLOR_CHECK_LEGACY_WHITE UINT32_C(1)
#define INKPOD_COLOR_CHECK_NATIVE_ALPHA UINT32_C(2)

/** @brief 永続化される typed layer の種類。 */
typedef uint32_t InkpodLayerKind;
#define INKPOD_LAYER_BINARY_COLORING UINT32_C(1)
#define INKPOD_LAYER_GRAYSCALE_COLORING UINT32_C(2)
#define INKPOD_LAYER_RASTER UINT32_C(3)
#define INKPOD_LAYER_SELECTION UINT32_C(4)
#define INKPOD_LAYER_FRAME UINT32_C(5)
#define INKPOD_LAYER_VANISHING_POINT UINT32_C(6)
#define INKPOD_LAYER_ADJUSTMENT UINT32_C(7)
#define INKPOD_LAYER_TEXT UINT32_C(8)
#define INKPOD_LAYER_ANNOTATION UINT32_C(9)
#define INKPOD_LAYER_VECTOR_COLORING UINT32_C(10)

/** @brief layer 内の typed plane の種類。 */
typedef uint32_t InkpodTypedPlaneKind;
#define INKPOD_TYPED_PLANE_MAIN_LINE UINT32_C(1)
#define INKPOD_TYPED_PLANE_COLOR UINT32_C(2)
#define INKPOD_TYPED_PLANE_RASTER UINT32_C(3)
#define INKPOD_TYPED_PLANE_SELECTION UINT32_C(4)
#define INKPOD_TYPED_PLANE_VECTOR_MAIN_LINE UINT32_C(5)
#define INKPOD_TYPED_PLANE_COLOR_TRACE UINT32_C(6)
#define INKPOD_TYPED_PLANE_VECTOR_FILL UINT32_C(7)
#define INKPOD_EDIT_TARGET_LAYER UINT32_C(1)
#define INKPOD_EDIT_TARGET_PLANE UINT32_C(2)
#define INKPOD_EDIT_TARGET_DUPLICATE UINT32_C(1)
#define INKPOD_EDIT_TARGET_DELETE UINT32_C(2)
#define INKPOD_EDIT_TARGET_SET_VISIBILITY UINT32_C(3)
#define INKPOD_EDIT_TARGET_SET_EDITABILITY UINT32_C(4)
#define INKPOD_EDIT_TARGET_CONVERT_PLANES UINT32_C(5)
#define INKPOD_EDIT_TARGET_CONVERT_LAYERS UINT32_C(6)
#define INKPOD_EDIT_TARGET_MERGE UINT32_C(7)
#define INKPOD_MAX_EDIT_TARGETS UINT32_C(4096)

#define INKPOD_VECTOR_PATH_CLOSED (UINT64_C(1) << 0)
/** @brief vector 消去範囲のモード型。 */
typedef uint32_t InkpodVectorEraseMode;
#define INKPOD_VECTOR_ERASE_PARTIAL UINT32_C(1)
#define INKPOD_VECTOR_ERASE_TO_INTERSECTION UINT32_C(2)
#define INKPOD_VECTOR_ERASE_WHOLE_PATH UINT32_C(3)
/** @brief vector 線幅補正の演算型。 */
typedef uint32_t InkpodVectorWidthMode;
#define INKPOD_VECTOR_WIDTH_ADD UINT32_C(1)
#define INKPOD_VECTOR_WIDTH_SUBTRACT UINT32_C(2)
#define INKPOD_VECTOR_WIDTH_SCALE UINT32_C(3)
#define INKPOD_VECTOR_WIDTH_CONSTANT UINT32_C(4)
/** @brief vector object 選択規則の型。 */
typedef uint32_t InkpodVectorSelectionMode;
#define INKPOD_VECTOR_SELECT_CUT_BY_SELECTION UINT32_C(1)
#define INKPOD_VECTOR_SELECT_TOUCHING UINT32_C(2)
#define INKPOD_VECTOR_SELECT_FULLY_CONTAINED UINT32_C(3)
#define INKPOD_VECTOR_SELECT_LINE UINT32_C(4)
#define INKPOD_VECTOR_SELECT_WHOLE_LINE UINT32_C(5)
#define INKPOD_VECTOR_SELECT_TO_INTERSECTION UINT32_C(6)
#define INKPOD_VECTOR_SELECT_FILL_BOUNDARY UINT32_C(7)
#define INKPOD_VECTOR_SELECT_FILL UINT32_C(8)
#define INKPOD_VECTOR_RASTERIZE_ANTIALIAS (UINT64_C(1) << 0)
#define INKPOD_SNAPSHOT_VECTOR_CLOSED (UINT32_C(1) << 0)
#define INKPOD_SNAPSHOT_VECTOR_STROKE_VISIBLE (UINT32_C(1) << 1)

/** @brief Semantic operation in an ordered immutable snapshot render plan. */
typedef uint32_t InkpodRenderPassKind;
#define INKPOD_RENDER_PASS_LAYER_BEGIN UINT32_C(1)
#define INKPOD_RENDER_PASS_RASTER_TILES UINT32_C(2)
#define INKPOD_RENDER_PASS_VECTOR_FILLS UINT32_C(3)
#define INKPOD_RENDER_PASS_VECTOR_STROKES UINT32_C(4)
#define INKPOD_RENDER_PASS_ADJUSTMENT UINT32_C(5)
#define INKPOD_RENDER_PASS_LAYER_END UINT32_C(6)

/** @brief filter catalog の処理識別子型。 */
typedef uint32_t InkpodFilterKind;
#define INKPOD_FILTER_SHARPEN_WEAK UINT32_C(1)
#define INKPOD_FILTER_SHARPEN_STRONG UINT32_C(2)
#define INKPOD_FILTER_BLUR_WEAK UINT32_C(3)
#define INKPOD_FILTER_BLUR_STRONG UINT32_C(4)
#define INKPOD_FILTER_GAUSSIAN_BLUR UINT32_C(5)
#define INKPOD_FILTER_INVERT UINT32_C(6)
#define INKPOD_FILTER_AUTO_CONTRAST UINT32_C(7)
#define INKPOD_FILTER_BRIGHTNESS_CONTRAST UINT32_C(8)
#define INKPOD_FILTER_TONE_CURVE UINT32_C(9)
#define INKPOD_FILTER_LEVELS UINT32_C(10)
#define INKPOD_FILTER_HSV UINT32_C(11)
#define INKPOD_FILTER_COLOR_BALANCE UINT32_C(12)
#define INKPOD_FILTER_UNSHARP_MASK UINT32_C(13)

/** @brief filter が対象とする RGB/channel 型。 */
typedef uint32_t InkpodFilterChannel;
#define INKPOD_FILTER_CHANNEL_RGB UINT32_C(1)
#define INKPOD_FILTER_CHANNEL_RED UINT32_C(2)
#define INKPOD_FILTER_CHANNEL_GREEN UINT32_C(3)
#define INKPOD_FILTER_CHANNEL_BLUE UINT32_C(4)

/** @brief tone curve の補間方式型。 */
typedef uint32_t InkpodCurveInterpolation;
#define INKPOD_CURVE_BEZIER UINT32_C(1)
#define INKPOD_CURVE_BSPLINE UINT32_C(2)

/** @brief linear/radial gradient の種類。 */
typedef uint32_t InkpodGradientKind;
#define INKPOD_GRADIENT_LINEAR UINT32_C(1)
#define INKPOD_GRADIENT_RADIAL UINT32_C(2)

/** @brief gradient の composite/overwrite モード型。 */
typedef uint32_t InkpodGradientMode;
#define INKPOD_GRADIENT_COMPOSITE UINT32_C(1)
#define INKPOD_GRADIENT_OVERWRITE UINT32_C(2)
#define INKPOD_GRADIENT_FLAG_CONSTRAIN_45 (UINT64_C(1) << 0)

#define INKPOD_EFFECT_FLAG_PRESSURE_SIZE (UINT64_C(1) << 0)
#define INKPOD_EFFECT_FLAG_PRESSURE_OPACITY (UINT64_C(1) << 1)

/** @brief stamp gesture の形状型。 */
typedef uint32_t InkpodStampShape;
#define INKPOD_STAMP_ROUND UINT32_C(1)
#define INKPOD_STAMP_SQUARE UINT32_C(2)

/** @brief dust removal algorithm のモード型。 */
typedef uint32_t InkpodDustMode;
#define INKPOD_DUST_REMOVE_FOREGROUND UINT32_C(1)
#define INKPOD_DUST_FILL_TRANSPARENT_HOLES UINT32_C(2)
#define INKPOD_DUST_REPLACE_COLOR_OUTLIERS UINT32_C(3)

/** @brief thread-safe task の進行状態型。 */
typedef uint32_t InkpodTaskState;
#define INKPOD_TASK_READY UINT32_C(0)
#define INKPOD_TASK_RUNNING UINT32_C(1)
#define INKPOD_TASK_COMPLETED UINT32_C(2)
#define INKPOD_TASK_CANCELLED UINT32_C(3)
#define INKPOD_TASK_FAILED UINT32_C(4)

#define INKPOD_BATCH_GRAPH_VERSION UINT32_C(2)
/** @brief batch graph の入力 selector 種類。 */
typedef uint32_t InkpodBatchInputKind;
#define INKPOD_BATCH_INPUT_FILE UINT32_C(1)
#define INKPOD_BATCH_INPUT_FOLDER UINT32_C(2)
#define INKPOD_BATCH_INPUT_CURRENT_SEQUENCE UINT32_C(3)
/** @brief batch output の重複／上書き方針型。 */
typedef uint32_t InkpodBatchOutputPolicy;
#define INKPOD_BATCH_OUTPUT_DUPLICATE UINT32_C(1)
#define INKPOD_BATCH_OUTPUT_NEW_SAVE UINT32_C(2)
#define INKPOD_BATCH_OUTPUT_EXPLICIT_OVERWRITE UINT32_C(3)
/** @brief batch item 失敗後の続行／停止方針型。 */
typedef uint32_t InkpodBatchFailurePolicy;
#define INKPOD_BATCH_FAILURE_CONTINUE UINT32_C(1)
#define INKPOD_BATCH_FAILURE_STOP UINT32_C(2)
/** @brief batch target が存在しない場合の方針型。 */
typedef uint32_t InkpodBatchMissingPolicy;
#define INKPOD_BATCH_MISSING_SKIP UINT32_C(1)
#define INKPOD_BATCH_MISSING_ERROR UINT32_C(2)
/** @brief batch graph 内 operation の識別子型。 */
typedef uint32_t InkpodBatchOperationKind;
#define INKPOD_BATCH_OPERATION_COLOR_REPLACE UINT32_C(1)
#define INKPOD_BATCH_OPERATION_CONTINUOUS_FILL UINT32_C(2)
#define INKPOD_BATCH_OPERATION_SEPARATION UINT32_C(3)
#define INKPOD_BATCH_OPERATION_VISIBILITY UINT32_C(4)
#define INKPOD_BATCH_OPERATION_LINE_WIDTH UINT32_C(5)
#define INKPOD_BATCH_OPERATION_FILTER UINT32_C(6)
#define INKPOD_BATCH_OPERATION_BOUNDARY_AIRBRUSH UINT32_C(7)
#define INKPOD_BATCH_OPERATION_DUST_REMOVAL UINT32_C(8)
#define INKPOD_BATCH_OPERATION_MIRROR UINT32_C(9)
#define INKPOD_BATCH_OPERATION_ROTATE_90 UINT32_C(10)
#define INKPOD_BATCH_OPERATION_RESIZE UINT32_C(11)
#define INKPOD_BATCH_OPERATION_CONVERT_PLANE UINT32_C(12)
#define INKPOD_BATCH_OPERATION_ENABLED UINT64_C(1)
#define INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN (UINT64_C(1) << 1)
#define INKPOD_BATCH_OUTPUT_CELL_FOLDER UINT64_C(1)
#define INKPOD_BATCH_OUTPUT_DESCENDING (UINT64_C(1) << 1)
#define INKPOD_BATCH_OUTPUT_PREVIEW_BEFORE_SAVE (UINT64_C(1) << 2)
#define INKPOD_BATCH_SEPARATION_INVERT INT64_C(1)
#define INKPOD_BATCH_SEED_HAS_EXPECTED_COLOR UINT32_C(1)
#define INKPOD_BATCH_SEED_ENABLED (UINT32_C(1) << 1)
#define INKPOD_BATCH_SEPARATION_REPLACE_SOURCE INT64_C(1)
#define INKPOD_BATCH_SEPARATION_SELECTION_MASK INT64_C(2)
#define INKPOD_BATCH_SEPARATION_MAIN_LINE_PLANE INT64_C(3)
#define INKPOD_BATCH_SEPARATION_COLOR_PLANE INT64_C(4)
#define INKPOD_BATCH_SEPARATION_NATIVE_FILE INT64_C(5)
/** @brief batch 実行対象を current/all から選ぶ型。 */
typedef uint32_t InkpodBatchRunScope;
#define INKPOD_BATCH_SCOPE_CURRENT UINT32_C(1)
#define INKPOD_BATCH_SCOPE_ALL UINT32_C(2)
#define INKPOD_BATCH_RUN_DRY UINT64_C(1)
#define INKPOD_BATCH_RUN_PREVIEW_CONFIRMED (UINT64_C(1) << 1)
/** @brief batch report item の結果型。 */
typedef uint32_t InkpodBatchItemOutcome;
#define INKPOD_BATCH_ITEM_SUCCEEDED UINT32_C(1)
#define INKPOD_BATCH_ITEM_SKIPPED UINT32_C(2)
#define INKPOD_BATCH_ITEM_FAILED UINT32_C(3)
#define INKPOD_BATCH_ITEM_CANCELLED UINT32_C(4)
#define INKPOD_BATCH_ITEM_DRY_RUN UINT32_C(5)
#define INKPOD_BATCH_PREVIEW_HAS_WARNING UINT32_C(1)

/** @brief 永続 raster/input raster の straight-alpha storage format 型。 */
typedef uint32_t InkpodStoragePixelFormat;
#define INKPOD_STORAGE_BINARY8 UINT32_C(1)
#define INKPOD_STORAGE_GRAYSCALE8 UINT32_C(2)
#define INKPOD_STORAGE_GRAYSCALE16 UINT32_C(3)
#define INKPOD_STORAGE_RGBA8 UINT32_C(4)
#define INKPOD_STORAGE_RGBA16 UINT32_C(5)

/** @brief light-table item の表示変換モード型。 */
typedef uint32_t InkpodLightTableDisplayMode;
#define INKPOD_LIGHT_TABLE_COLOR UINT32_C(1)
#define INKPOD_LIGHT_TABLE_MONOTONE UINT32_C(2)
#define INKPOD_LIGHT_TABLE_HALFTONE UINT32_C(3)
#define INKPOD_LIGHT_TABLE_ITEM_VISIBLE (UINT32_C(1) << 0)
#define INKPOD_LIGHT_TABLE_SET_ACTIVE (UINT32_C(1) << 1)
#define INKPOD_LIGHT_TABLE_CREATE_SET UINT32_C(1)
#define INKPOD_LIGHT_TABLE_DUPLICATE_SET UINT32_C(2)
#define INKPOD_LIGHT_TABLE_DELETE_SET UINT32_C(3)
#define INKPOD_LIGHT_TABLE_RENAME_SET UINT32_C(4)
#define INKPOD_LIGHT_TABLE_REORDER_SET UINT32_C(5)
#define INKPOD_LIGHT_TABLE_SET_ACTIVE_OPERATION UINT32_C(6)
#define INKPOD_LIGHT_TABLE_REMOVE_ITEM UINT32_C(7)
#define INKPOD_LIGHT_TABLE_REORDER_ITEM UINT32_C(8)
#define INKPOD_LIGHT_TABLE_UPDATE_ITEM UINT32_C(9)

/** @brief sequence/motion-check の移動方向型。 */
typedef uint32_t InkpodSequenceDirection;
#define INKPOD_SEQUENCE_PREVIOUS UINT32_C(1)
#define INKPOD_SEQUENCE_NEXT UINT32_C(2)
#define INKPOD_SEQUENCE_FLAG_LOOP (UINT32_C(1) << 0)
#define INKPOD_MOTION_FLAG_LOOP (UINT64_C(1) << 0)
#define INKPOD_MOTION_FLAG_INCLUDE_SELECTION (UINT64_C(1) << 1)
#define INKPOD_MOTION_FLAG_INCLUDE_LIGHT_TABLE (UINT64_C(1) << 2)
#define INKPOD_MOTION_FRAME_PAUSED (UINT32_C(1) << 0)
#define INKPOD_MOTION_FRAME_INCLUDE_SELECTION (UINT32_C(1) << 1)
#define INKPOD_MOTION_FRAME_INCLUDE_LIGHT_TABLE (UINT32_C(1) << 2)

/** @brief layer/plane tree 編集 operation の識別子型。 */
typedef uint32_t InkpodTreeOperation;
#define INKPOD_TREE_CREATE_LAYER UINT32_C(1)
#define INKPOD_TREE_DUPLICATE_LAYER UINT32_C(2)
#define INKPOD_TREE_DELETE_LAYER UINT32_C(3)
#define INKPOD_TREE_REORDER_LAYER UINT32_C(4)
#define INKPOD_TREE_SET_LAYER_PROPERTIES UINT32_C(5)
#define INKPOD_TREE_CREATE_PLANE UINT32_C(6)
#define INKPOD_TREE_DUPLICATE_PLANE UINT32_C(7)
#define INKPOD_TREE_DELETE_PLANE UINT32_C(8)
#define INKPOD_TREE_REORDER_PLANE UINT32_C(9)
#define INKPOD_TREE_SET_PLANE_PROPERTIES UINT32_C(10)
#define INKPOD_TREE_CONVERT_LAYER UINT32_C(11)
#define INKPOD_TREE_MERGE_LAYER UINT32_C(12)
#define INKPOD_TREE_CONVERT_PLANE UINT32_C(13)
#define INKPOD_TREE_MERGE_PLANE UINT32_C(14)
#define INKPOD_TREE_DELETE_HIDDEN_LAYERS UINT32_C(15)
#define INKPOD_NODE_VISIBLE (UINT64_C(1) << 0)
#define INKPOD_NODE_EDITABLE (UINT64_C(1) << 1)

/** @brief raster selection/region の形状型。 */
typedef uint32_t InkpodSelectionShape;
#define INKPOD_SELECTION_RECTANGLE UINT32_C(1)
#define INKPOD_SELECTION_ELLIPSE UINT32_C(2)
#define INKPOD_SELECTION_LASSO UINT32_C(3)
#define INKPOD_SELECTION_POLYLINE UINT32_C(4)
#define INKPOD_SELECTION_TRACE UINT32_C(5)
#define INKPOD_SELECTION_WAND UINT32_C(6)
/** @brief Explicit raster/vector topology for scoped exact color replacement. */
typedef uint32_t InkpodScopedColorReplaceMode;
#define INKPOD_COLOR_REPLACE_RASTER_COLOR UINT32_C(1)
#define INKPOD_COLOR_REPLACE_RASTER_MAIN_LINE UINT32_C(2)
#define INKPOD_COLOR_REPLACE_VECTOR_COLOR_LINE UINT32_C(3)
#define INKPOD_COLOR_REPLACE_VECTOR_MAIN_LINE UINT32_C(4)
#define INKPOD_COLOR_REPLACE_VECTOR_FILL UINT32_C(5)
#define INKPOD_COLOR_REPLACE_HAS_REGION (UINT64_C(1) << 0)
#define INKPOD_COLOR_REPLACE_FLAGS INKPOD_COLOR_REPLACE_HAS_REGION
#define INKPOD_COLOR_REPLACE_PREVIEW_HAS_BOUNDS (UINT32_C(1) << 0)
/** @brief geometric candidate に適用する raster 内容解釈。 */
typedef uint32_t InkpodRangeInterpretation;
#define INKPOD_RANGE_NORMAL UINT32_C(1)
#define INKPOD_RANGE_TIGHT UINT32_C(2)
#define INKPOD_RANGE_ENCLOSED_INTERIOR UINT32_C(3)
#define INKPOD_RANGE_DRAWING UINT32_C(4)
#define INKPOD_RANGE_BOUNDARY UINT32_C(5)
/** @brief trace brush の stamp 形状。 */
typedef uint32_t InkpodTraceBrushShape;
#define INKPOD_TRACE_ROUND UINT32_C(1)
#define INKPOD_TRACE_SQUARE UINT32_C(2)
#define INKPOD_SELECTION_FROM_CENTER (UINT64_C(1) << 0)
#define INKPOD_SELECTION_CONSTRAIN_ROTATION_45 (UINT64_C(1) << 1)
#define INKPOD_SELECTION_TRACE_PRESSURE_SIZE (UINT64_C(1) << 2)
#define INKPOD_SELECTION_TRACE_SCREEN_SIZE (UINT64_C(1) << 3)
#define INKPOD_SELECTION_CONSTRUCTION_FLAGS ((UINT64_C(1) << 4) - UINT64_C(1))
/** @brief selection mask の new/add/subtract/intersect 演算型。 */
typedef uint32_t InkpodSelectionOperation;
#define INKPOD_SELECTION_NEW UINT32_C(1)
#define INKPOD_SELECTION_ADD UINT32_C(2)
#define INKPOD_SELECTION_SUBTRACT UINT32_C(3)
#define INKPOD_SELECTION_INTERSECT UINT32_C(4)
#define INKPOD_SELECTION_ADJUST_INVERT UINT32_C(1)
#define INKPOD_SELECTION_ADJUST_EXPAND UINT32_C(2)
#define INKPOD_SELECTION_ADJUST_SHRINK UINT32_C(3)
#define INKPOD_SELECTION_LAYER_REPLACE UINT32_C(1)
#define INKPOD_SELECTION_LAYER_ADD UINT32_C(2)
#define INKPOD_SELECTION_LAYER_SUBTRACT UINT32_C(3)

#define INKPOD_GUIDE_HORIZONTAL UINT32_C(1)
#define INKPOD_GUIDE_VERTICAL UINT32_C(2)
#define INKPOD_MIRROR_HORIZONTAL UINT32_C(1)
#define INKPOD_MIRROR_VERTICAL UINT32_C(2)
#define INKPOD_ROTATE_LEFT_90 UINT32_C(1)
#define INKPOD_ROTATE_RIGHT_90 UINT32_C(2)
#define INKPOD_RESIZE_ANCHOR_TOP_LEFT UINT32_C(1)
#define INKPOD_RESIZE_ANCHOR_TOP_RIGHT UINT32_C(2)
#define INKPOD_RESIZE_ANCHOR_CENTER UINT32_C(3)
#define INKPOD_RESIZE_ANCHOR_BOTTOM_LEFT UINT32_C(4)
#define INKPOD_RESIZE_ANCHOR_BOTTOM_RIGHT UINT32_C(5)
#define INKPOD_DOCUMENT_RESIZE_RESAMPLE (UINT64_C(1) << 0)
#define INKPOD_PASTE_COMPATIBLE UINT32_C(1)
#define INKPOD_PASTE_ACTIVE_CONVERTED UINT32_C(2)

/** @brief 作成スレッドに固定された Rust-owned Core opaque handle。 */
typedef struct InkpodCore InkpodCore;
/** @brief Rust-owned immutable validated blank-cell creation plan. */
typedef struct InkpodCellCreationPlan InkpodCellCreationPlan;
/** @brief Core から独立して生存できる immutable・Rust-owned snapshot handle。 */
typedef struct InkpodSnapshot InkpodSnapshot;
/** @brief typed clipboard payload を保持する immutable・Rust-owned handle。 */
typedef struct InkpodClipboard InkpodClipboard;
/** @brief encode 結果の immutable byte 列を保持する Rust-owned handle。 */
typedef struct InkpodByteBuffer InkpodByteBuffer;
/** @brief sequence の複数 encode 結果を保持する Rust-owned handle。 */
typedef struct InkpodEncodedSequence InkpodEncodedSequence;
/** @brief 任意スレッドから query/cancel できる Rust-owned atomic task handle。 */
typedef struct InkpodTask InkpodTask;
/** @brief `InkpodTask` と同じ ABI・所有権を持つ batch task 別名。 */
typedef InkpodTask InkpodBatchTask;
/** @brief コピー済み batch 設定を保持する immutable・Rust-owned handle。 */
typedef struct InkpodBatchGraph InkpodBatchGraph;
/** @brief Rust-owned immutable `.inkchart` current-version decode result. */
typedef struct InkpodColorChartFile InkpodColorChartFile;
/** @brief batch preview item を所有する immutable・Rust-owned handle。 */
typedef struct InkpodBatchPreview InkpodBatchPreview;
/** @brief Rust-owned immutable two-cell exact color-pair extraction result. */
typedef struct InkpodBatchPairPreview InkpodBatchPairPreview;
/** @brief batch report item を所有する immutable・Rust-owned handle。 */
typedef struct InkpodBatchReport InkpodBatchReport;

/** @brief Core 作成時の ABI version と feature を渡す入力。全体を borrowed で読み取る。 */
typedef struct InkpodCoreConfig {
    uint32_t struct_size;
    uint32_t abi_version;
    uint64_t feature_flags;
} InkpodCoreConfig;

#define INKPOD_DIGEST_BLAKE3_256 UINT32_C(1)

/** @brief Build-time procedure replay contract. */
typedef struct InkpodReplayContract {
    uint32_t struct_size;
    uint32_t replay_epoch;
    uint32_t procedure_format_version;
    uint32_t canonical_numeric_version;
    uint32_t primitive_count;
    uint32_t reserved;
    uint64_t feature_flags;
    uint8_t primitive_catalog_digest[32];
} InkpodReplayContract;

/** @brief Canonical architecture-independent 256-bit digest. */
typedef struct InkpodCanonicalDigest {
    uint32_t struct_size;
    uint32_t algorithm;
    uint8_t bytes[32];
} InkpodCanonicalDigest;

/** @brief 成功した編集後の revision と受理件数を受け取る caller-owned 出力。 */
typedef struct InkpodDispatchResult {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t revision;
    uint64_t accepted_command_count;
} InkpodDispatchResult;

/** @brief Deterministic native persistence and checkpoint-policy diagnostics. */
typedef struct InkpodPersistenceInfo {
    uint32_t struct_size;
    uint32_t format_version;
    uint32_t open_strategy;
    uint32_t flags;
    uint64_t feature_flags;
    uint64_t journal_event_count;
    uint64_t procedure_count;
    uint64_t replay_work;
    uint64_t dirty_bytes;
    uint64_t asset_count;
    uint64_t asset_bytes;
} InkpodPersistenceInfo;

/** @brief Exact confirmation token for a separate history-losing compacted copy. */
typedef struct InkpodCompactionPlan {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t history_event_count;
    uint64_t history_procedure_count;
    uint8_t document_digest[32];
    uint8_t editor_digest[32];
    uint8_t journal_digest[32];
} InkpodCompactionPlan;

/** @brief 新規 Cell の UUID、寸法、DPI を指定する borrowed 入力。 */
typedef struct InkpodCellCreateOptions {
    uint32_t struct_size;
    /** `INKPOD_CELL_CREATE_INITIAL_LAYER_KIND`時だけtyped layer kind、それ以外は0。 */
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t document_uuid_high;
    uint64_t document_uuid_low;
    uint32_t width;
    uint32_t height;
    uint32_t dpi_x_milli;
    uint32_t dpi_y_milli;
} InkpodCellCreateOptions;
#define INKPOD_CELL_CREATE_INITIAL_LAYER_KIND (UINT64_C(1) << 0)

#define INKPOD_CELL_SIZING_IMAGE_PIXELS UINT32_C(1)
#define INKPOD_CELL_SIZING_FRAME_MICROMETRES UINT32_C(2)
#define INKPOD_FRAME_ANCHOR_TOP_LEFT UINT32_C(1)
#define INKPOD_FRAME_ANCHOR_TOP_RIGHT UINT32_C(2)
#define INKPOD_FRAME_ANCHOR_CENTER UINT32_C(3)
#define INKPOD_FRAME_ANCHOR_BOTTOM_LEFT UINT32_C(4)
#define INKPOD_FRAME_ANCHOR_BOTTOM_RIGHT UINT32_C(5)
#define INKPOD_MAX_CELL_CREATION_COUNT UINT32_C(64)

/** @brief Complete bounded input for an immutable multi-cell creation plan. */
typedef struct InkpodCellCreationOptions {
    uint32_t struct_size;
    uint32_t sizing_mode;
    uint64_t feature_flags;
    uint32_t width;
    uint32_t height;
    uint32_t dpi_x_milli;
    uint32_t dpi_y_milli;
    uint32_t margin_milli;
    uint32_t safe_frame_ratio_milli;
    uint32_t maximum_close_ratio_milli;
    uint32_t anchor;
    InkpodLayerKind initial_layer_kind;
    uint32_t pixel_format;
    uint32_t count;
    uint32_t reserved;
} InkpodCellCreationOptions;

/** @brief document 座標の符号付き矩形。`struct_size` を持たない値型。 */
typedef struct InkpodFrameRect {
    int32_t x;
    int32_t y;
    int32_t width;
    int32_t height;
} InkpodFrameRect;

/** @brief Caller-owned copy of one immutable cell creation plan item. */
typedef struct InkpodCellCreationPlanItem {
    uint32_t struct_size;
    uint32_t sizing_mode;
    uint32_t width;
    uint32_t height;
    uint32_t dpi_x_milli;
    uint32_t dpi_y_milli;
    InkpodLayerKind initial_layer_kind;
    uint32_t pixel_format;
    InkpodFrameRect hundred_frame;
    InkpodFrameRect reference_frame;
    InkpodFrameRect drawing_frame;
    InkpodFrameRect safe_frame;
    InkpodFrameRect shooting_frame;
    InkpodFrameRect maximum_close_frame;
    uint32_t margin_left;
    uint32_t margin_top;
    uint32_t margin_right;
    uint32_t margin_bottom;
} InkpodCellCreationPlanItem;

/** @brief 文書・view revision、dirty/history flags、stable ID と紙情報の caller-owned 出力。 */
typedef struct InkpodDocumentInfo {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t document_revision;
    uint64_t view_revision;
    uint64_t document_id;
    uint64_t document_uuid_high;
    uint64_t document_uuid_low;
    uint64_t layer_id;
    uint64_t main_plane_id;
    uint64_t color_plane_id;
    uint32_t width;
    uint32_t height;
    uint32_t dpi_x_milli;
    uint32_t dpi_y_milli;
    InkpodFrameRect hundred_frame;
    InkpodFrameRect reference_frame;
    InkpodFrameRect drawing_frame;
    InkpodFrameRect safe_frame;
    InkpodFrameRect shooting_frame;
    InkpodFrameRect maximum_close_frame;
    uint32_t margin_left;
    uint32_t margin_top;
    uint32_t margin_right;
    uint32_t margin_bottom;
    InkpodPlaneKind active_plane;
    uint32_t reserved;
    uint64_t main_plane_checksum;
    uint64_t color_plane_checksum;
} InkpodDocumentInfo;

/**
 * @brief Core owner-thread session の deterministic な論理 resource 使用量。
 *
 * tile/history byte は payload の論理保持量であり、copy-on-write clone 間で共有される
 * allocator/driver private resident size を推測しない。query は snapshot、revision、dirty、
 * history、savepoint を変更しない。
 */
typedef struct InkpodResourceUsage {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t document_tile_bytes;
    uint64_t document_tile_count;
    uint64_t history_bytes;
    uint64_t history_entry_count;
    uint64_t render_cache_bytes;
    uint64_t render_cache_tile_count;
    uint64_t cpu_staging_bytes;
    uint64_t reference_light_table_bytes;
    uint64_t reference_light_table_tile_count;
    uint64_t sequence_source_bytes;
    uint64_t sequence_source_tile_count;
    uint64_t thumbnail_cache_bytes;
} InkpodResourceUsage;

/** @brief 100F/基準/作画/安全/撮影/最大クローズ frame と margin を更新する borrowed 入力。 */
typedef struct InkpodPaperFramesInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    InkpodFrameRect hundred_frame;
    InkpodFrameRect reference_frame;
    InkpodFrameRect drawing_frame;
    InkpodFrameRect safe_frame;
    InkpodFrameRect shooting_frame;
    InkpodFrameRect maximum_close_frame;
    uint32_t margin_left;
    uint32_t margin_top;
    uint32_t margin_right;
    uint32_t margin_bottom;
} InkpodPaperFramesInput;

/** @brief history cursor と総 item 数の caller-owned 出力。 */
typedef struct InkpodHistoryInfo {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t cursor;
    uint64_t item_count;
} InkpodHistoryInfo;

/** @brief history item と UTF-8 名を caller-owned buffer へ受け取る size-query 対応出力。 */
typedef struct InkpodHistoryItem {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t index;
    uint8_t* name_utf8;
    uint64_t name_capacity;
    uint64_t name_bytes;
} InkpodHistoryItem;

/** @brief 位置・筆圧を持つ stroke sample record。span の stride ごとに borrowed で読む。 */
typedef struct InkpodStrokeSample {
    uint32_t struct_size;
    uint32_t flags;
    float x;
    float y;
    float pressure;
    uint32_t reserved;
} InkpodStrokeSample;

/**
 * @brief stroke の tool/style と初期 sample span を渡す borrowed 入力。
 * `samples` は `sample_count > 0` なら非 NULL かつ各 record の `struct_size` が必要。
 * `shape` は ROUND/SQUARE、`smoothing` は 0..1000、`start_color` は
 * ANY/EXACT_NATIVE。reserved fields は 0 とし、Core は call 復帰後に入力を保持しない。
 */
typedef struct InkpodStrokeInput {
    uint32_t struct_size;
    InkpodPaintTool tool;
    InkpodPlaneKind plane;
    InkpodCoordinateSpace coordinate_space;
    uint64_t flags;
    uint32_t color_rgba; /**< 0xRRGGBBAA の straight-alpha sRGB。 */
    float diameter;
    const InkpodStrokeSample* samples;
    uint64_t sample_count;
    uint64_t sample_stride_bytes;
    InkpodBrushShape shape;
    uint16_t smoothing; /**< 0=off、1..1000=Core-owned fixed-point smoothing。 */
    uint16_t reserved_2;
    InkpodStartColorPredicate start_color;
    uint32_t reserved_3;
} InkpodStrokeInput;

/** @brief live stroke へ追加する caller-owned sample span。呼び出し終了後は保持しない。 */
typedef struct InkpodStrokeSampleSpan {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    const InkpodStrokeSample* samples;
    uint64_t sample_count;
    uint64_t sample_stride_bytes;
} InkpodStrokeSampleSpan;

/** @brief view command とその 4 値を渡す borrowed 入力。文書内容を所有しない。 */
typedef struct InkpodViewInput {
    uint32_t struct_size;
    InkpodViewCommandKind kind;
    uint64_t flags;
    double value1;
    double value2;
    double value3;
    double value4;
} InkpodViewInput;

/** @brief depth を明示した straight-alpha 色値。8-bit 値は各 channel の下位 8 bit を使う。 */
typedef struct InkpodColorValue {
    uint32_t struct_size;
    InkpodColorDepth depth;
    uint16_t red;
    uint16_t green;
    uint16_t blue;
    uint16_t alpha;
} InkpodColorValue;

/**
 * @brief 一つの Core generation に属する Rust-owned object の value identity。
 *
 * `generation + object_type + value` の全組で identity を表す。別 Core、解放済み、
 * type違いの ID は受理しない。`NONE/0/0` は所有権を持たない空 ID である。
 */
typedef struct InkpodObjectId {
    uint32_t struct_size;
    InkpodObjectType object_type;
    uint64_t feature_flags;
    uint64_t generation;
    uint64_t value;
} InkpodObjectId;

/** @brief pointer/path/callbackを含まないABI-v3 canonical primitive request。 */
typedef struct InkpodPrimitiveRequestV3 {
    uint32_t struct_size;
    InkpodPrimitiveOpcode opcode;
    uint32_t schema_version;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t base_revision;
    uint64_t target_id;
    InkpodObjectId payload_id;
    InkpodPaintTool tool;
    InkpodPlaneKind plane;
    InkpodCoordinateSpace coordinate_space;
    uint32_t reserved_2;
    uint64_t stroke_flags;
    InkpodColorValue color;
    float diameter;
    uint32_t reserved_3;
} InkpodPrimitiveRequestV3;

/** @brief canonical primitive のcommit/no-op結果。 */
typedef struct InkpodPrimitiveResultV3 {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t revision;
    uint64_t accepted_command_count;
    uint64_t procedure_id;
    uint64_t committed_state_id;
    InkpodPrimitiveOpcode opcode;
    uint32_t schema_version;
} InkpodPrimitiveResultV3;

/** @brief 1 bounded call中だけborrowし、canonical dense rasterへコピーする入力。 */
typedef struct InkpodRasterAssetInputV3 {
    uint32_t struct_size;
    InkpodStoragePixelFormat pixel_format;
    uint64_t feature_flags;
    uint32_t width;
    uint32_t height;
    uint32_t reserved;
    uint32_t reserved_2;
    uint64_t row_stride_bytes;
    const uint8_t* pixels;
    uint64_t pixel_bytes;
} InkpodRasterAssetInputV3;

/** @brief live ABI-v3 object のpointer-free metadata。 */
typedef struct InkpodObjectInfoV3 {
    uint32_t struct_size;
    InkpodObjectType object_type;
    uint64_t feature_flags;
    uint64_t generation;
    uint64_t value;
    uint64_t element_count;
    uint64_t byte_count;
    uint32_t width;
    uint32_t height;
    uint64_t stride_bytes;
    uint64_t revision;
} InkpodObjectInfoV3;

/** @brief live snapshot ID のpointer-free metadata。 */
typedef struct InkpodSnapshotInfoV3 {
    uint32_t struct_size;
    uint32_t transform_flags;
    uint64_t feature_flags;
    uint64_t revision;
    uint64_t view_revision;
    uint64_t tile_count;
    uint64_t guide_count;
    uint64_t vector_segment_count;
    uint64_t vector_fill_count;
    uint64_t vector_boundary_path_count;
    double zoom;
    double pan_x;
    double pan_y;
    uint32_t document_width;
    uint32_t document_height;
} InkpodSnapshotInfoV3;

/** @brief batched copyで返すpointer-free snapshot tile descriptor。 */
typedef struct InkpodSnapshotTileInfoV3 {
    uint32_t struct_size;
    InkpodPixelFormat pixel_format;
    uint64_t tile_id;
    int32_t origin_x;
    int32_t origin_y;
    uint32_t width;
    uint32_t height;
    uint32_t stride_bytes;
    uint32_t reserved;
    uint64_t pixel_bytes;
    uint64_t tile_revision;
} InkpodSnapshotTileInfoV3;

/** @brief caller-owned byte storageへのoffset付きbounded copy。 */
typedef struct InkpodBufferCopyV3 {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t offset;
    uint8_t* bytes;
    uint64_t byte_capacity;
    uint64_t written_bytes;
    uint64_t total_bytes;
} InkpodBufferCopyV3;

/** @brief exact-depth color record の caller-owned borrowed span。 */
typedef struct InkpodColorArray {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    const InkpodColorValue* colors;
    uint64_t color_count;
    uint64_t color_stride_bytes;
} InkpodColorArray;

/** @brief color を受け取る caller-owned size-query 対応 buffer。Core は所有しない。 */
typedef struct InkpodColorBuffer {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    InkpodColorValue* colors;
    uint64_t color_capacity;
    uint64_t color_stride_bytes;
    uint64_t color_count;
} InkpodColorBuffer;

/** @brief `.inkchart` save に渡す exact-depth color と borrowed UTF-8 name。 */
typedef struct InkpodColorChartEntry {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    InkpodColorValue color;
    const uint8_t* name_utf8;
    uint64_t name_bytes;
} InkpodColorChartEntry;

/** @brief fill tool の Core-owned option を exact-depth 色と共にコピーする値 record。 */
typedef struct InkpodEditorFillOptions {
    uint32_t struct_size;
    InkpodFillOperation operation;
    uint64_t flags;
    uint16_t tolerance;
    uint16_t gap_close;
    InkpodInclusionMode inclusion_mode;
    uint32_t extension_distance;
    uint32_t inclusion_color_count;
    uint32_t reserved;
    InkpodColorValue inclusion_colors[INKPOD_EDITOR_MAX_INCLUSION_COLORS];
} InkpodEditorFillOptions;

/** @brief selection tool の shape/operation/range/geometry/trace option をコピーする値 record。 */
typedef struct InkpodEditorSelectionOptions {
    uint32_t struct_size;
    InkpodSelectionShape shape;
    InkpodSelectionOperation operation;
    uint32_t reserved;
    uint16_t tolerance;
    uint16_t gap_close;
    uint32_t reserved2;
    int64_t diameter_q16;
    InkpodRangeInterpretation interpretation;
    uint32_t aspect_ratio_q16;
    uint64_t construction_flags;
    uint32_t rotation_turns;
    InkpodTraceBrushShape trace_shape;
} InkpodEditorSelectionOptions;

/** @brief vector erase/select option をコピーする値 record。 */
typedef struct InkpodEditorVectorOptions {
    uint32_t struct_size;
    InkpodVectorEraseMode erase_mode;
    InkpodVectorSelectionMode selection_mode;
    uint32_t reserved;
} InkpodEditorVectorOptions;

/**
 * @brief Core-owned raster brush options copied by value.
 *
 * `shape` is ROUND or SQUARE, `smoothing` is 0..1000, and `start_color` is ANY or
 * EXACT_NATIVE. Reserved fields must be zero. EXACT_NATIVE includes alpha and compares
 * the immutable pre-stroke pixel in its native channel depth without requiring connectivity.
 * Caller storage is borrowed only for the update call and is never retained.
 */
typedef struct InkpodEditorBrushOptions {
    uint32_t struct_size;
    InkpodBrushShape shape;
    uint16_t smoothing;
    uint16_t reserved;
    InkpodStartColorPredicate start_color;
    uint32_t reserved2;
} InkpodEditorBrushOptions;

/**
 * @brief document session に属する Core-owned EditorState の caller-owned snapshot。
 *
 * `editor_revision` と `editor_digest` は意味変更だけで進み、document revision/history/render
 * とは独立する。色は `InkpodColorValue` の depth を保持し packed RGBA8 へ縮小しない。
 */
typedef struct InkpodEditorStateInfo {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t feature_flags;
    uint64_t editor_revision;
    uint8_t editor_digest[32];
    InkpodEditorTool active_tool;
    InkpodEditorTool last_color_consuming_tool;
    InkpodColorValue current_color;
    uint32_t reserved;
    int64_t current_diameter_q16;
    uint64_t active_layer_id;
    uint64_t active_plane_id;
    uint32_t palette_group;
    uint32_t palette_index;
    InkpodEditorFillOptions fill;
    InkpodEditorSelectionOptions selection;
    InkpodEditorVectorOptions vector;
    InkpodEditorBrushOptions brush;
} InkpodEditorStateInfo;

/** @brief document 作成前にも取得できる immutable built-in defaults のコピー。 */
typedef struct InkpodEditorDefaults {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint32_t width;
    uint32_t height;
    uint32_t dpi_x_milli;
    uint32_t dpi_y_milli;
    InkpodEditorStateInfo state;
} InkpodEditorDefaults;

/**
 * @brief `kind` が選ぶ一つの EditorState field update。
 *
 * 入力は呼出中だけ borrowed。未選択 field は無視される。`expected_editor_revision` は
 * query で得た対象 session の厳密な base revision でなければならない。
 */
typedef struct InkpodEditorStateUpdate {
    uint32_t struct_size;
    InkpodEditorUpdateKind kind;
    uint64_t expected_editor_revision;
    uint64_t flags;
    InkpodEditorTool tool;
    uint32_t reserved;
    InkpodColorValue color;
    int64_t diameter_q16;
    uint64_t active_layer_id;
    uint64_t active_plane_id;
    uint32_t palette_group;
    uint32_t palette_index;
    InkpodEditorFillOptions fill;
    InkpodEditorSelectionOptions selection;
    InkpodEditorVectorOptions vector;
    InkpodEditorBrushOptions brush;
} InkpodEditorStateUpdate;

/**
 * @brief Core-owned EditorStateからtool/color/diameter/target/brush optionsを開始時にcaptureするstroke入力。
 *
 * sample spanは呼出中だけborrowedでCoreが全値をcopyする。`tool == 0` はactive tool、
 * 非0は指定toolのCore-owned styleを選ぶ。callerはcolor/diameter/target/brush optionsを渡さず、
 * `inkpod_core_editor_stroke_begin`が対象sessionのexact-depth値を一度だけ確定する。
 */
typedef struct InkpodEditorStrokeInput {
    uint32_t struct_size;
    InkpodCoordinateSpace coordinate_space;
    InkpodEditorTool tool; /**< 0=active tool、それ以外=指定Core-owned tool style。 */
    uint32_t reserved;
    uint64_t flags;
    const InkpodStrokeSample* samples;
    uint64_t sample_count;
    uint64_t sample_stride_bytes;
} InkpodEditorStrokeInput;

/** Core-owned grouped edit target. Caller storage is never retained. */
typedef struct InkpodEditTarget {
    uint32_t struct_size;
    uint32_t kind;
    uint64_t layer_id;
    uint64_t plane_id; /**< layer target では 0。 */
    uint64_t reserved;
} InkpodEditTarget;

/** One grouped layer/plane command. `flags` is the boolean value for SET operations. */
typedef struct InkpodEditTargetCommand {
    uint32_t struct_size;
    uint32_t operation;
    uint64_t flags;
    uint32_t kind;
    uint32_t pixel_format;
    uint64_t reserved;
} InkpodEditTargetCommand;

/** @brief 現在の有効な編集対象集合に対する副作用のない capability matrix。 */
typedef struct InkpodEditTargetCapabilities {
    uint32_t struct_size;
    uint32_t can_duplicate;
    uint32_t can_delete;
    uint32_t can_set_visibility;
    uint32_t can_set_editability;
    uint32_t can_merge;
    uint32_t can_convert_planes;
    uint32_t can_convert_layers;
    uint32_t reserved;
} InkpodEditTargetCapabilities;

/**
 * @brief fill の演算、色、seed、選択範囲、包含色 span を渡す borrowed 入力。
 * inclusion span は count 0 のときだけ NULL/stride 0 を許す。
 */
typedef struct InkpodFillInput {
    uint32_t struct_size;
    InkpodFillOperation operation;
    uint64_t flags;
    uint32_t seed_x;
    uint32_t seed_y;
    InkpodColorValue color;
    uint16_t tolerance; /**< channel 差の正規化 16-bit 上限。 */
    uint16_t gap_close;
    InkpodInclusionMode inclusion_mode;
    InkpodFrameRect selection;
    const InkpodColorValue* inclusion_colors;
    uint64_t inclusion_color_count;
    uint64_t inclusion_color_stride_bytes; /**< count 0 の場合だけ 0 可。 */
    uint32_t extension_distance;
    uint32_t reserved;
} InkpodFillInput;

/** @brief fill の revision、変更 pixel 数、漏れ候補を受け取る caller-owned 出力。 */
typedef struct InkpodFillResult {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t revision;
    uint64_t changed_pixel_count;
    uint32_t leak_x;
    uint32_t leak_y;
} InkpodFillResult;

/** @brief snapshot 構築 option。ABI v1 では既知 feature を指定する borrowed 入力。 */
typedef struct InkpodSnapshotOptions {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
} InkpodSnapshotOptions;

/**
 * @brief immutable snapshot 内の 1 tile を記述する borrowed record。
 * `pixels` は親 `InkpodSnapshot` の release まで有効で、個別解放しない。
 */
typedef struct InkpodSnapshotTile {
    uint32_t struct_size;
    InkpodPixelFormat pixel_format;
    uint64_t tile_id;
    int32_t origin_x;
    int32_t origin_y;
    uint32_t width;
    uint32_t height;
    uint32_t stride_bytes;
    uint32_t reserved;
    const uint8_t* pixels;
    uint64_t pixel_bytes;
    uint64_t tile_revision;
} InkpodSnapshotTile;

/**
 * @brief snapshot の tile span を返す caller-owned view record。
 * `tiles` と各 pixel span は親 snapshot 所有で、その生存中だけ borrowed。
 */
typedef struct InkpodSnapshotView {
    uint32_t struct_size;
    uint32_t abi_version;
    uint64_t feature_flags;
    uint64_t revision;
    const InkpodSnapshotTile* tiles;
    uint64_t tile_count;
    uint64_t tile_stride_bytes;
} InkpodSnapshotView;

/** @brief snapshot が固定した zoom/pan/flip と view revision の caller-owned コピー出力。 */
typedef struct InkpodSnapshotTransform {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t view_revision;
    double zoom;
    double pan_x;
    double pan_y;
    uint32_t document_width;
    uint32_t document_height;
} InkpodSnapshotTransform;

/** @brief snapshot 内 guide の immutable borrowed record。 */
typedef struct InkpodSnapshotGuide {
    uint32_t struct_size;
    uint32_t axis;
    int32_t position;
    uint32_t reserved;
    uint64_t id;
} InkpodSnapshotGuide;

/**
 * @brief overlay flags/grid と guide span を返す caller-owned view record。
 * guide span は親 snapshot の release まで borrowed。
 */
typedef struct InkpodSnapshotOverlay {
    uint32_t struct_size;
    uint32_t flags;
    int32_t grid_origin_x;
    int32_t grid_origin_y;
    uint32_t grid_spacing_x;
    uint32_t grid_spacing_y;
    uint32_t grid_subdivisions;
    uint32_t reserved;
    const InkpodSnapshotGuide* guides;
    uint64_t guide_count;
    uint64_t guide_stride_bytes;
} InkpodSnapshotOverlay;

/** @brief document logical 座標の 2 次元点。`struct_size` を持たない値型。 */
typedef struct InkpodVectorPoint {
    float x;
    float y;
} InkpodVectorPoint;

/** @brief cubic control points と両端線幅を持つ borrowed vector segment record。 */
typedef struct InkpodVectorCubicSegment {
    uint32_t struct_size;
    uint32_t reserved;
    InkpodVectorPoint p0;
    InkpodVectorPoint p1;
    InkpodVectorPoint p2;
    InkpodVectorPoint p3;
    float width_start;
    float width_end;
} InkpodVectorCubicSegment;

/** @brief 1 vector path と segment span を作成する borrowed 入力。Core は値をコピーする。 */
typedef struct InkpodVectorPathInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t flags;
    uint64_t plane_id;
    InkpodColorValue color;
    const InkpodVectorCubicSegment* segments;
    uint64_t segment_count;
    uint64_t segment_stride_bytes;
} InkpodVectorPathInput;

/** @brief fill color と閉じた boundary path ID span を渡す borrowed 入力。 */
typedef struct InkpodVectorFillInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t plane_id;
    InkpodColorValue color;
    const uint64_t* boundary_path_ids;
    uint64_t boundary_path_count;
} InkpodVectorFillInput;

/** @brief vector erase の plane、位置、半径、mode を渡す borrowed 入力。 */
typedef struct InkpodVectorEraseInput {
    uint32_t struct_size;
    InkpodVectorEraseMode mode;
    uint64_t plane_id;
    float x;
    float y;
    float radius;
    uint32_t reserved;
} InkpodVectorEraseInput;

/** @brief path ID span と線幅補正演算を渡す borrowed 入力。 */
typedef struct InkpodVectorWidthInput {
    uint32_t struct_size;
    InkpodVectorWidthMode mode;
    uint64_t feature_flags;
    const uint64_t* path_ids;
    uint64_t path_count;
    float parameter;
    uint32_t reserved;
} InkpodVectorWidthInput;

/** @brief vector 選択 mode と document bounds を渡す borrowed 入力。 */
typedef struct InkpodVectorSelectionInput {
    uint32_t struct_size;
    InkpodVectorSelectionMode mode;
    uint64_t feature_flags;
    InkpodFrameRect bounds;
} InkpodVectorSelectionInput;

/** @brief path 上の選択区間を 0..1,000,000 で表す caller-owned 出力 record。 */
typedef struct InkpodVectorSelectionRange {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t path_id;
    uint32_t start_million;
    uint32_t end_million;
} InkpodVectorSelectionRange;

/** @brief 選択 range と fill ID を受け取る caller-owned count-query 対応 buffer。 */
typedef struct InkpodVectorSelectionBuffer {
    uint32_t struct_size;
    uint32_t reserved;
    InkpodVectorSelectionRange* ranges;
    uint64_t range_capacity;
    uint64_t range_count;
    uint64_t* fill_ids;
    uint64_t fill_capacity;
    uint64_t fill_count;
} InkpodVectorSelectionBuffer;

/** @brief vector layer の rasterize scale/flags を渡す borrowed 入力。 */
typedef struct InkpodVectorRasterizeInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t layer_id;
    uint32_t scale;
    uint32_t reserved_2;
} InkpodVectorRasterizeInput;

/** @brief straight RGBA8 raster を受け取る caller-owned size-query 対応 buffer。 */
typedef struct InkpodVectorRasterBuffer {
    uint32_t struct_size;
    uint32_t reserved;
    uint8_t* pixels;
    uint64_t pixel_capacity;
    uint64_t required_bytes;
    uint32_t width;
    uint32_t height;
    uint32_t stride_bytes;
    uint32_t reserved_2;
} InkpodVectorRasterBuffer;

/** @brief raster plane から vector layer へ変換する閾値と stable ID の borrowed 入力。 */
typedef struct InkpodRasterVectorizeInput {
    uint32_t struct_size;
    uint32_t alpha_threshold;
    uint64_t feature_flags;
    uint64_t source_plane_id;
    uint64_t target_layer_id;
} InkpodRasterVectorizeInput;

/** @brief 正規化 16-bit 入出力値を持つ tone-curve point record。 */
typedef struct InkpodCurvePoint {
    uint32_t struct_size;
    uint32_t reserved;
    uint32_t input;
    uint32_t output;
} InkpodCurvePoint;

/**
 * @brief filter 種類、対象 plane、channel、parameter と curve span の borrowed 入力。
 *
 * parameter は正規化 milli-unit。Gaussian は parameter_0/1 を radius/strength、
 * brightness/contrast は parameter_0/1、levels は parameter_0..4 を
 * input shadow/gamma/highlight と output shadow/highlight に使う。
 */
typedef struct InkpodFilterInput {
    uint32_t struct_size;
    InkpodFilterKind kind;
    uint64_t feature_flags;
    uint64_t plane_id;
    InkpodFilterChannel channel;
    InkpodCurveInterpolation interpolation;
    int32_t parameter_0;
    int32_t parameter_1;
    int32_t parameter_2;
    int32_t parameter_3;
    int32_t parameter_4;
    /**
     * curve point の record stride。非空で 0 は packed-v1 互換として受理するが、
     * 新規 caller は `sizeof(*points)` を渡す。`reserved` は初期 ABI v1 の綴りとの
     * source compatibility のために残す。
     */
    union {
        uint32_t point_stride_bytes;
        uint32_t reserved;
    };
    const InkpodCurvePoint* points;
    uint64_t point_count;
} InkpodFilterInput;

/** @brief filter preview の対象 plane、base/preview checksum、transient revision 出力。 */
typedef struct InkpodFilterPreviewInfo {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t plane_id;
    uint64_t base_checksum;
    uint64_t preview_checksum;
    uint64_t preview_revision;
} InkpodFilterPreviewInfo;

/** @brief 0..1000 の位置と exact-depth 色を持つ gradient stop record。 */
typedef struct InkpodGradientStop {
    uint32_t struct_size;
    uint32_t reserved;
    uint32_t position_milli;
    uint32_t reserved_2;
    InkpodColorValue color;
} InkpodGradientStop;

/** @brief gradient geometry、mode と caller-owned stop span を渡す borrowed 入力。 */
typedef struct InkpodGradientInput {
    uint32_t struct_size;
    InkpodGradientKind kind;
    uint64_t feature_flags;
    uint64_t plane_id;
    InkpodGradientMode mode;
    uint32_t dither;
    int64_t start_x_milli;
    int64_t start_y_milli;
    int64_t end_x_milli;
    int64_t end_y_milli;
    const InkpodGradientStop* stops;
    uint64_t stop_count;
    uint64_t stop_stride_bytes;
} InkpodGradientInput;

/** @brief 1 回の primitive airbrush dab を指定する borrowed 入力。 */
typedef struct InkpodAirbrushInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t plane_id;
    int64_t center_x_milli;
    int64_t center_y_milli;
    uint32_t radius_milli;
    uint32_t hardness_milli;
    uint32_t opacity_milli;
    uint32_t reserved_2;
    InkpodColorValue color;
} InkpodAirbrushInput;

/** @brief 境界 airbrush の幅、強度、色 span を指定する borrowed 入力。 */
typedef struct InkpodBoundaryAirbrushInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t plane_id;
    uint32_t width;
    uint32_t strength_milli;
    InkpodColorArray colors;
} InkpodBoundaryAirbrushInput;

/** @brief plane 全体／選択範囲へ blur primitive を適用する borrowed 入力。 */
typedef struct InkpodBlurEffectInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t plane_id;
    uint32_t radius;
    uint32_t strength_milli;
    uint32_t reserved_2;
    uint32_t reserved_3;
} InkpodBlurEffectInput;

/** @brief source rectangle を destination へ複製する stamp primitive 入力。 */
typedef struct InkpodStampInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t plane_id;
    int32_t source_x;
    int32_t source_y;
    int32_t destination_x;
    int32_t destination_y;
    uint32_t width;
    uint32_t height;
    uint32_t opacity_milli;
    uint32_t reserved_2;
} InkpodStampInput;

/**
 * @brief target alpha を置換する grayscale8/16 raster の borrowed 入力。
 *
 * `pixels` は呼び出し中だけ借用する。row padding は許すが、target 寸法と一致し、
 * advertised byte range に全 row が収まらなければならない。
 */
typedef struct InkpodAlphaEditInput {
    uint32_t struct_size;
    InkpodStoragePixelFormat pixel_format;
    uint64_t feature_flags;
    uint64_t plane_id;
    uint32_t width;
    uint32_t height;
    uint32_t reserved;
    uint32_t reserved_2;
    const uint8_t* pixels;
    uint64_t pixel_bytes;
    uint64_t row_stride_bytes;
} InkpodAlphaEditInput;

/**
 * @brief airbrush gesture の style と sample span を渡す borrowed 入力。
 *
 * 座標は document logical または client device pixel。`view_id == 0` は primary view。
 * sample は呼び出し中にコピーされ、完了 gesture は高々 1 Undo 単位になる。
 */
typedef struct InkpodAirbrushGestureInput {
    uint32_t struct_size;
    InkpodCoordinateSpace coordinate_space;
    uint64_t feature_flags;
    uint64_t plane_id;
    uint64_t view_id;
    uint32_t radius_milli;
    uint32_t hardness_milli;
    uint32_t spacing_milli;
    uint32_t opacity_milli;
    uint32_t fade_milli;
    uint32_t continuous_dabs;
    InkpodColorValue color;
    const InkpodStrokeSample* samples;
    uint64_t sample_count;
    uint64_t sample_stride_bytes;
} InkpodAirbrushGestureInput;

/** @brief stamp source と destination sample span を持つ borrowed gesture 入力。 */
typedef struct InkpodStampGestureInput {
    uint32_t struct_size;
    InkpodCoordinateSpace coordinate_space;
    uint64_t feature_flags;
    uint64_t plane_id;
    uint64_t view_id;
    InkpodStrokeSample source;
    uint32_t radius_milli;
    uint32_t hardness_milli;
    uint32_t spacing_milli;
    uint32_t opacity_milli;
    InkpodStampShape shape;
    uint32_t reserved;
    const InkpodStrokeSample* samples;
    uint64_t sample_count;
    uint64_t sample_stride_bytes;
} InkpodStampGestureInput;

/** @brief atomic task の状態と completed/total work を受け取る caller-owned 出力。 */
typedef struct InkpodTaskInfo {
    uint32_t struct_size;
    InkpodTaskState state;
    uint64_t completed_work;
    uint64_t total_work;
    uint64_t reserved;
} InkpodTaskInfo;

/**
 * @brief batch graph の file/folder/current-sequence selector 入力。
 * path span は `inkpod_batch_graph_create` 中だけ borrowed で、graph へコピーされる。
 */
typedef struct InkpodBatchInput {
    uint32_t struct_size;
    InkpodBatchInputKind kind;
    uint64_t feature_flags;
    const uint8_t* path_utf8;
    uint64_t path_bytes;
    uint32_t first_cell;
    uint32_t last_cell;
    uint64_t reserved;
} InkpodBatchInput;

/** @brief batch color replacement の old/new exact-depth 色 pair record。 */
typedef struct InkpodBatchColorPairInput {
    uint32_t struct_size;
    uint32_t enabled;
    uint64_t reserved;
    InkpodColorValue old_color;
    InkpodColorValue new_color;
} InkpodBatchColorPairInput;

/** @brief continuous fill の行単位 enable、seed、許容差、期待色を持つ borrowed record。 */
typedef struct InkpodBatchSeedInput {
    uint32_t struct_size;
    uint32_t flags;
    uint32_t x;
    uint32_t y;
    uint32_t tolerance;
    uint32_t gap_close;
    uint64_t reserved;
    InkpodColorValue fill_color;
    InkpodColorValue expected_color;
} InkpodBatchSeedInput;

/**
 * @brief kind ごとの parameter と nested span を持つ versioned batch operation 入力。
 *
 * visibility [0]=0/1、line width [0]=mode/[1]=value*1000、
 * separation [0]=`INKPOD_BATCH_SEPARATION_INVERT` または 0、[1] は
 * `INKPOD_BATCH_SEPARATION_*` typed destination、seed flags は
 * `INKPOD_BATCH_SEED_ENABLED` と任意の期待色検査を持つ。
 * boundary effect [0]=width/[1]=strength_milli、dust [0]=`InkpodDustMode`/[1]=maximum_pixels、
 * mirror/rotate [0]=axis/direction、resize [0..5]=width,height,dpi_x,dpi_y,resample,anchor、
 * convert [0..1]=`InkpodTypedPlaneKind`,`InkpodStoragePixelFormat`。
 * 全 nested pointer は graph 作成中だけ borrowed で、成功時に graph が値をコピーする。
 */
typedef struct InkpodBatchOperationInput {
    uint32_t struct_size;
    uint32_t version;
    InkpodBatchOperationKind kind;
    uint32_t reserved;
    uint64_t flags;
    uint64_t layer_id;
    uint64_t plane_id;
    InkpodLayerKind layer_kind;
    InkpodTypedPlaneKind plane_kind;
    InkpodBatchMissingPolicy missing_policy;
    uint32_t reserved_2;
    int64_t parameters[8];
    InkpodColorValue color_0;
    InkpodColorValue color_1;
    InkpodColorArray colors;
    const InkpodFilterInput* filter;
    const InkpodBatchColorPairInput* color_pairs;
    uint64_t color_pair_count;
    uint64_t color_pair_stride_bytes;
    const InkpodBatchSeedInput* seeds;
    uint64_t seed_count;
    uint64_t seed_stride_bytes;
    uint64_t reserved_3;
} InkpodBatchOperationInput;

/**
 * @brief batch の入力、ordered operation、出力方針と UTF-8 名をまとめる borrowed 入力。
 * 成功時は全 nested span が immutable `InkpodBatchGraph` へコピーされる。
 */
typedef struct InkpodBatchGraphInput {
    uint32_t struct_size;
    uint32_t version;
    uint64_t feature_flags;
    const uint8_t* name_utf8;
    uint64_t name_bytes;
    const InkpodBatchInput* inputs;
    uint64_t input_count;
    uint64_t input_stride_bytes;
    const InkpodBatchOperationInput* operations;
    uint64_t operation_count;
    uint64_t operation_stride_bytes;
    InkpodBatchOutputPolicy output_policy;
    InkpodBatchFailurePolicy failure_policy;
    uint64_t output_flags;
    const uint8_t* output_folder_utf8;
    uint64_t output_folder_bytes;
    const uint8_t* basename_utf8;
    uint64_t basename_bytes;
    uint32_t start_number;
    uint32_t wait_milliseconds;
    uint64_t reserved;
} InkpodBatchGraphInput;

/** @brief immutable batch graph の version/count/policy を受け取る caller-owned コピー出力。 */
typedef struct InkpodBatchGraphInfo {
    uint32_t struct_size;
    uint32_t version;
    uint64_t input_count;
    uint64_t operation_count;
    InkpodBatchOutputPolicy output_policy;
    InkpodBatchFailurePolicy failure_policy;
    uint64_t output_flags;
} InkpodBatchGraphInfo;

/**
 * @brief immutable graph 内の一操作を caller-owned scalar/count record へコピーする。
 * nested colors/pairs/seeds/curve points は対応する indexed query で取得する。
 */
typedef struct InkpodBatchOperationInfo {
    uint32_t struct_size;
    uint32_t version;
    InkpodBatchOperationKind kind;
    uint32_t reserved;
    uint64_t flags;
    uint64_t layer_id;
    uint64_t plane_id;
    InkpodLayerKind layer_kind;
    InkpodTypedPlaneKind plane_kind;
    InkpodBatchMissingPolicy missing_policy;
    uint32_t reserved_2;
    int64_t parameters[8];
    InkpodColorValue color_0;
    InkpodColorValue color_1;
    uint32_t filter_kind;
    uint32_t filter_channel;
    uint32_t filter_interpolation;
    uint32_t reserved_3;
    int32_t filter_parameters[5];
    uint32_t reserved_4;
    uint64_t color_count;
    uint64_t color_pair_count;
    uint64_t seed_count;
    uint64_t curve_point_count;
} InkpodBatchOperationInfo;

/**
 * @brief batch preview 1 item の borrowed UTF-8 span を返す view record。
 * 文字列は親 `InkpodBatchPreview` の release まで有効で、個別解放しない。
 */
typedef struct InkpodBatchPreviewItem {
    uint32_t struct_size;
    uint32_t flags;
    const uint8_t* input_name;
    uint64_t input_name_bytes;
    const uint8_t* output_path;
    uint64_t output_path_bytes;
    const uint8_t* warning;
    uint64_t warning_bytes;
} InkpodBatchPreviewItem;

/** @brief batch report の cancelled/item/failure count を受け取る caller-owned 出力。 */
typedef struct InkpodBatchReportInfo {
    uint32_t struct_size;
    uint32_t cancelled;
    uint64_t item_count;
    uint64_t failure_count;
    uint64_t reserved;
} InkpodBatchReportInfo;

/**
 * @brief batch report 1 item の outcome と borrowed UTF-8 span を返す view record。
 * 文字列は親 `InkpodBatchReport` の release まで有効。
 */
typedef struct InkpodBatchReportItem {
    uint32_t struct_size;
    InkpodBatchItemOutcome outcome;
    const uint8_t* input_name;
    uint64_t input_name_bytes;
    const uint8_t* output_path;
    uint64_t output_path_bytes;
    const uint8_t* message;
    uint64_t message_bytes;
} InkpodBatchReportItem;

/** @brief Exact Core-owned sequence raster identity used for two-cell comparison. */
typedef struct InkpodSequenceSourceIdentity {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t document_uuid_high;
    uint64_t document_uuid_low;
    uint64_t source_generation;
} InkpodSequenceSourceIdentity;

/** @brief Geometry, native format, and bounded counts for a pair preview. */
typedef struct InkpodBatchPairPreviewInfo {
    uint32_t struct_size;
    InkpodStoragePixelFormat pixel_format;
    uint32_t width;
    uint32_t height;
    uint32_t ambiguity_count;
    uint32_t reserved;
    uint64_t candidate_count;
    uint64_t unchanged_pixel_count;
} InkpodBatchPairPreviewInfo;

#define INKPOD_BATCH_PAIR_CANDIDATE_AMBIGUOUS UINT32_C(1)

/** @brief One exact old/new candidate; colors include straight alpha. */
typedef struct InkpodBatchPairCandidate {
    uint32_t struct_size;
    uint32_t flags;
    InkpodColorValue old_color;
    InkpodColorValue new_color;
    uint64_t pixel_count;
    int32_t bounds_x;
    int32_t bounds_y;
    int32_t bounds_width;
    int32_t bounds_height;
} InkpodBatchPairCandidate;

/** @brief snapshot 内の path/plane/order/color/cubic/width を持つ borrowed segment record。 */
typedef struct InkpodSnapshotVectorSegment {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t path_id;
    uint64_t plane_id;
    uint32_t z_order;
    uint32_t segment_index;
    uint32_t segment_count;
    uint32_t color_rgba;
    InkpodVectorPoint p0;
    InkpodVectorPoint p1;
    InkpodVectorPoint p2;
    InkpodVectorPoint p3;
    float width_start;
    float width_end;
} InkpodSnapshotVectorSegment;

/** @brief snapshot 内 vector fill と boundary ID 範囲を持つ borrowed record。 */
typedef struct InkpodSnapshotVectorFill {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t fill_id;
    uint64_t plane_id;
    uint32_t z_order;
    uint32_t color_rgba;
    uint64_t first_boundary_path;
    uint64_t boundary_path_count;
} InkpodSnapshotVectorFill;

/**
 * @brief snapshot の segment/fill/boundary-ID span を返す caller-owned view record。
 * すべての span は親 snapshot の release まで borrowed。
 */
typedef struct InkpodSnapshotVectorView {
    uint32_t struct_size;
    uint32_t abi_version;
    uint64_t feature_flags;
    const InkpodSnapshotVectorSegment* segments;
    uint64_t segment_count;
    uint64_t segment_stride_bytes;
    const InkpodSnapshotVectorFill* fills;
    uint64_t fill_count;
    uint64_t fill_stride_bytes;
    const uint64_t* boundary_path_ids;
    uint64_t boundary_path_count;
} InkpodSnapshotVectorView;

/** @brief Explicitly disconnected vector endpoint in stable path/plane identity order. */
typedef struct InkpodSnapshotVectorEndpoint {
    uint32_t struct_size;
    InkpodVectorEndpointKind endpoint;
    uint64_t path_id;
    uint64_t plane_id;
    InkpodVectorPoint point;
} InkpodSnapshotVectorEndpoint;

/**
 * @brief View-local vector diagnostic flags and snapshot-owned endpoint span.
 *
 * Endpoint marker size is a renderer concern in device pixels. The Core emits
 * only exact topological disconnections and never infers a connection from
 * coordinate proximity.
 */
typedef struct InkpodSnapshotVectorDiagnostics {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t feature_flags;
    const InkpodSnapshotVectorEndpoint* endpoints;
    uint64_t endpoint_count;
    uint64_t endpoint_stride_bytes;
} InkpodSnapshotVectorDiagnostics;

/** @brief One bottom-to-top operation in a snapshot-owned render plan. */
typedef struct InkpodSnapshotRenderPass {
    uint32_t struct_size;
    InkpodRenderPassKind kind;
    uint64_t layer_id;
    uint64_t plane_id;
    uint32_t opacity_milli;
    uint32_t reserved;
    uint64_t first_item;
    uint64_t item_count;
} InkpodSnapshotRenderPass;

/**
 * @brief Borrowed ordered render passes and RGB8 adjustment lookup tables.
 *
 * `passes` and `adjustment_luts_rgb8` remain valid until the parent snapshot is
 * released. Each LUT stores red, green, and blue tables consecutively, with
 * 256 bytes per channel.
 */
typedef struct InkpodSnapshotRenderPlan {
    uint32_t struct_size;
    uint32_t abi_version;
    uint64_t feature_flags;
    const InkpodSnapshotRenderPass* passes;
    uint64_t pass_count;
    uint64_t pass_stride_bytes;
    const uint8_t* adjustment_luts_rgb8;
    uint64_t adjustment_lut_count;
    uint64_t adjustment_lut_stride_bytes;
} InkpodSnapshotRenderPlan;

/** @brief tree operation、stable ID、properties、UTF-8 名を渡す borrowed 入力。 */
typedef struct InkpodTreeEdit {
    uint32_t struct_size;
    InkpodTreeOperation operation;
    uint64_t flags;
    uint64_t object_id;
    uint64_t parent_id;
    uint32_t destination_index;
    uint32_t kind;
    InkpodStoragePixelFormat pixel_format;
    uint32_t opacity_milli;
    const uint8_t* name_utf8;
    uint64_t name_bytes;
} InkpodTreeEdit;

/** @brief layer/plane metadata と UTF-8 名を caller-owned buffer へ返す size-query 対応出力。 */
typedef struct InkpodNodeInfo {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t id;
    uint64_t parent_id;
    uint32_t kind;
    InkpodStoragePixelFormat pixel_format;
    uint32_t opacity_milli;
    uint32_t index;
    uint32_t child_count;
    uint32_t reserved;
    uint8_t* name_utf8;
    uint64_t name_capacity;
    uint64_t name_bytes;
} InkpodNodeInfo;

/**
 * @brief 1 layer の縦横比維持 straight RGBA8 thumbnail を受け取る caller-owned buffer。
 * `maximum_width`/`maximum_height` は 1..256。`pixels_rgba8` は上から下へ packed される。
 */
typedef struct InkpodLayerThumbnailBuffer {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t layer_id;
    uint32_t maximum_width;
    uint32_t maximum_height;
    uint32_t width;
    uint32_t height;
    uint32_t stride_bytes;
    uint32_t reserved_2;
    uint64_t revision;
    uint8_t* pixels_rgba8;
    uint64_t pixel_capacity;
    uint64_t required_bytes;
} InkpodLayerThumbnailBuffer;

/** @brief selection path の document 座標点と正規化 pressure record。 */
typedef struct InkpodSelectionPoint {
    uint32_t struct_size;
    uint32_t reserved;
    float x;
    float y;
    float pressure;
    uint32_t reserved2;
} InkpodSelectionPoint;

/** @brief selection shape/operation、range、geometry、trace、wand 条件を渡す borrowed 入力。 */
typedef struct InkpodSelectionInput {
    uint32_t struct_size;
    InkpodSelectionShape shape;
    InkpodSelectionOperation operation;
    uint32_t reserved;
    InkpodFrameRect bounds;
    const InkpodSelectionPoint* points;
    uint64_t point_count;
    uint64_t point_stride_bytes;
    float diameter;
    uint16_t tolerance;
    uint16_t gap_close;
    uint32_t seed_x;
    uint32_t seed_y;
    InkpodRangeInterpretation interpretation;
    uint32_t aspect_ratio_q16;
    uint64_t construction_flags;
    uint32_t rotation_turns;
    InkpodTraceBrushShape trace_shape;
    int64_t view_zoom_q16;
} InkpodSelectionInput;

/**
 * @brief Borrowed, size-versioned scoped exact color replacement input.
 *
 * Colors retain native depth and alpha. With `INKPOD_COLOR_REPLACE_HAS_REGION`,
 * `shape` is rectangle, trace, polyline, or lasso and the optional point span is
 * borrowed only for the call. Without the flag every region field must be zero.
 * Core intersects a non-empty document selection with the region; no region and
 * no selection means the full document.
 */
typedef struct InkpodScopedColorReplaceInput {
    uint32_t struct_size;
    InkpodScopedColorReplaceMode mode;
    uint64_t feature_flags;
    uint64_t plane_id;
    uint64_t base_document_revision;
    InkpodColorValue target_color;
    InkpodColorValue replacement_color;
    InkpodSelectionShape shape;
    uint32_t reserved;
    InkpodFrameRect bounds;
    const InkpodSelectionPoint* points;
    uint64_t point_count;
    uint64_t point_stride_bytes;
    float diameter;
    uint32_t reserved_2;
} InkpodScopedColorReplaceInput;

/** @brief Caller-owned preview summary; no Rust allocation is transferred. */
typedef struct InkpodScopedColorReplacePreview {
    uint32_t struct_size;
    uint32_t feature_flags;
    uint64_t base_document_revision;
    uint64_t matched_pixels;
    uint64_t matched_objects;
    InkpodFrameRect affected_bounds;
} InkpodScopedColorReplacePreview;

/**
 * @brief blur tool の region/sample span を渡す borrowed 入力。
 * `INKPOD_EFFECT_FLAG_PRESSURE_SIZE` は pen region の直径にだけ適用できる。
 */
typedef struct InkpodBlurToolInput {
    uint32_t struct_size;
    InkpodCoordinateSpace coordinate_space;
    uint64_t feature_flags;
    uint64_t plane_id;
    uint64_t view_id;
    uint32_t radius;
    uint32_t strength_milli;
    InkpodSelectionShape shape;
    float diameter;
    const InkpodStrokeSample* samples;
    uint64_t sample_count;
    uint64_t sample_stride_bytes;
} InkpodBlurToolInput;

/** @brief dust mode、上限、任意 region/sample span を渡す borrowed 入力。 */
typedef struct InkpodDustInput {
    uint32_t struct_size;
    InkpodDustMode mode;
    uint64_t feature_flags;
    uint64_t plane_id;
    uint64_t view_id;
    InkpodCoordinateSpace coordinate_space;
    InkpodSelectionShape shape;
    uint32_t maximum_pixels;
    uint32_t use_region;
    float diameter;
    const InkpodStrokeSample* samples;
    uint64_t sample_count;
    uint64_t sample_stride_bytes;
} InkpodDustInput;

/** @brief floating paste の平行移動、拡縮、回転 preview 値を渡す borrowed 入力。 */
typedef struct InkpodFloatingTransform {
    uint32_t struct_size;
    uint32_t reserved;
    double translate_x;
    double translate_y;
    double scale_x;
    double scale_y;
    double rotation_degrees;
} InkpodFloatingTransform;

/** @brief document resize の寸法、DPI、anchor、resample flag を渡す borrowed 入力。 */
typedef struct InkpodDocumentResizeInput {
    uint32_t struct_size;
    uint32_t anchor;
    uint64_t flags;
    uint32_t width;
    uint32_t height;
    uint32_t dpi_x_milli;
    uint32_t dpi_y_milli;
} InkpodDocumentResizeInput;

/** @brief clipboard straight RGBA8 を受け取る caller-owned size-query 対応 buffer。 */
typedef struct InkpodClipboardRasterBuffer {
    uint32_t struct_size;
    uint32_t reserved;
    int32_t origin_x;
    int32_t origin_y;
    uint32_t width;
    uint32_t height;
    uint8_t* pixels_rgba8;
    uint64_t pixel_capacity;
    uint64_t required_bytes;
    uint64_t row_stride_bytes;
} InkpodClipboardRasterBuffer;

/**
 * @brief 外部 straight RGBA8 raster から clipboard を作る borrowed 入力。
 * padded row を許すが advertised byte range に全 row が必要。
 */
typedef struct InkpodClipboardRgbaInput {
    uint32_t struct_size;
    uint32_t reserved;
    int32_t origin_x;
    int32_t origin_y;
    uint32_t width;
    uint32_t height;
    const uint8_t* pixels_rgba8;
    uint64_t pixel_bytes;
    uint64_t row_stride_bytes;
} InkpodClipboardRgbaInput;

/** @brief document grid の origin、spacing、subdivision、flags を渡す borrowed 入力。 */
typedef struct InkpodGridInput {
    uint32_t struct_size;
    uint32_t reserved;
    int32_t origin_x;
    int32_t origin_y;
    uint32_t spacing_x;
    uint32_t spacing_y;
    uint32_t subdivisions;
    uint32_t flags;
} InkpodGridInput;

/** @brief device 点に対応する document 座標、任意 selection/color の caller-owned 出力。 */
typedef struct InkpodLocatorOutput {
    uint32_t struct_size;
    uint32_t flags;
    int32_t document_x;
    int32_t document_y;
    InkpodFrameRect selection;
    InkpodColorValue color;
} InkpodLocatorOutput;
#define INKPOD_LOCATOR_SELECTION_PRESENT (UINT32_C(1) << 0)
#define INKPOD_LOCATOR_COLOR_PRESENT (UINT32_C(1) << 1)

/** @brief locator 中心を含む caller-owned packed straight RGBA8 neighborhood 出力。 */
typedef struct InkpodLocatorNeighborhoodBuffer {
    uint32_t struct_size;
    uint32_t radius;
    uint32_t width;
    uint32_t height;
    int32_t origin_x;
    int32_t origin_y;
    uint32_t reserved;
    uint32_t reserved_2;
    uint8_t* pixels_rgba8;
    uint64_t pixel_capacity;
    uint64_t required_bytes;
} InkpodLocatorNeighborhoodBuffer;

/** @brief 一つの正規化済みshortcut stroke。 */
typedef struct InkpodShortcutStroke {
    uint32_t virtual_key;
    uint32_t modifiers;
} InkpodShortcutStroke;

/** @brief 最大4 strokeのprefix-free shortcut列。 */
typedef struct InkpodShortcutSequence {
    uint32_t struct_size;
    uint32_t command_id;
    uint32_t stroke_count;
    uint32_t reserved;
    InkpodShortcutStroke strokes[4];
} InkpodShortcutSequence;

/**
 * @brief light-table/sequence 用 straight RGBA8/16 raster の borrowed 入力。
 *
 * bytes は 1 呼び出し中だけ借用する。row padding は許すが、advertised byte range に
 * 全 row が必要。保持する API は戻る前に raster をコピーする。
 */
typedef struct InkpodRasterSourceInput {
    uint32_t struct_size;
    InkpodStoragePixelFormat pixel_format;
    uint64_t flags;
    uint64_t document_uuid_high;
    uint64_t document_uuid_low;
    uint64_t source_revision;
    uint32_t width;
    uint32_t height;
    uint32_t dpi_x_milli;
    uint32_t dpi_y_milli;
    InkpodFrameRect reference_frame;
    const uint8_t* pixels;
    uint64_t pixel_bytes;
    uint64_t row_stride_bytes;
} InkpodRasterSourceInput;

/** @brief light-table item の表示属性、UTF-8 名、source raster をまとめる borrowed 入力。 */
typedef struct InkpodLightTableItemInput {
    uint32_t struct_size;
    uint32_t flags;
    uint32_t opacity_milli;
    InkpodLightTableDisplayMode display_mode;
    InkpodColorValue display_color;
    int32_t translate_x_milli;
    int32_t translate_y_milli;
    uint32_t scale_x_milli;
    uint32_t scale_y_milli;
    int32_t rotation_milli_degrees;
    uint32_t reserved;
    const uint8_t* name_utf8;
    uint64_t name_bytes;
    InkpodRasterSourceInput source;
} InkpodLightTableItemInput;

/** @brief light-table set/item の operation と属性変更を渡す borrowed 入力。 */
typedef struct InkpodLightTableEdit {
    uint32_t struct_size;
    uint32_t operation;
    uint64_t object_id;
    uint32_t destination_index;
    uint32_t flags;
    uint32_t opacity_milli;
    InkpodLightTableDisplayMode display_mode;
    InkpodColorValue display_color;
    int32_t translate_x_milli;
    int32_t translate_y_milli;
    uint32_t scale_x_milli;
    uint32_t scale_y_milli;
    int32_t rotation_milli_degrees;
    uint32_t reserved;
    const uint8_t* name_utf8;
    uint64_t name_bytes;
} InkpodLightTableEdit;

/** @brief light-table set metadata と UTF-8 名の caller-owned size-query 対応出力。 */
typedef struct InkpodLightTableSetInfo {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t id;
    uint32_t opacity_milli;
    uint32_t item_count;
    uint8_t* name_utf8;
    uint64_t name_capacity;
    uint64_t name_bytes;
} InkpodLightTableSetInfo;

/** @brief light-table item の stable source/表示属性/名を返す caller-owned 出力。 */
typedef struct InkpodLightTableItemInfo {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t id;
    uint64_t source_plane_id;
    uint64_t source_document_uuid_high;
    uint64_t source_document_uuid_low;
    uint64_t source_revision;
    uint32_t opacity_milli;
    uint32_t effective_opacity_milli;
    InkpodLightTableDisplayMode display_mode;
    InkpodColorValue display_color;
    int32_t translate_x_milli;
    int32_t translate_y_milli;
    uint32_t scale_x_milli;
    uint32_t scale_y_milli;
    int32_t rotation_milli_degrees;
    uint32_t reserved;
    uint8_t* name_utf8;
    uint64_t name_capacity;
    uint64_t name_bytes;
} InkpodLightTableItemInfo;

/** @brief sequence cell の UTF-8 名と copied raster source を渡す borrowed record。 */
typedef struct InkpodSequenceCellInput {
    uint32_t struct_size;
    uint32_t reserved;
    const uint8_t* name_utf8;
    uint64_t name_bytes;
    InkpodRasterSourceInput source;
} InkpodSequenceCellInput;

/** @brief caller-owned sequence-cell strided span を渡す borrowed 入力。 */
typedef struct InkpodSequenceInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    const InkpodSequenceCellInput* cells;
    uint64_t cell_count;
    uint64_t cell_stride_bytes;
} InkpodSequenceInput;

/** @brief UTF-8 名と encoded file byte span を組にした borrowed record。 */
typedef struct InkpodNamedBytesInput {
    uint32_t struct_size;
    uint32_t reserved;
    const uint8_t* name_utf8;
    uint64_t name_bytes;
    const uint8_t* bytes;
    uint64_t byte_count;
} InkpodNamedBytesInput;

/** @brief UTF-8 名、common-raster format、encoded byte span の borrowed record。 */
typedef struct InkpodNamedRasterInput {
    uint32_t struct_size;
    uint32_t reserved;
    InkpodCommonRasterFormat format;
    uint32_t reserved2;
    const uint8_t* name_utf8;
    uint64_t name_bytes;
    const uint8_t* bytes;
    uint64_t byte_count;
} InkpodNamedRasterInput;

/** @brief sequence cell の UUID/番号/thumbnail/名を返す caller-owned size-query 対応出力。 */
typedef struct InkpodSequenceCellInfo {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t sequence_index;
    uint64_t document_uuid_high;
    uint64_t document_uuid_low;
    uint32_t cell_number;
    uint32_t width;
    uint32_t height;
    uint32_t thumbnail_width;
    uint32_t thumbnail_height;
    uint32_t reserved;
    uint64_t thumbnail_checksum;
    uint8_t* name_utf8;
    uint64_t name_capacity;
    uint64_t name_bytes;
} InkpodSequenceCellInfo;

/** @brief sequence thumbnail を受け取る caller-owned size-query 対応 buffer。 */
typedef struct InkpodSequenceThumbnailBuffer {
    uint32_t struct_size;
    uint32_t flags;
    uint32_t width;
    uint32_t height;
    uint32_t stride_bytes;
    uint32_t reserved;
    uint64_t checksum;
    uint8_t* pixels_rgba8;
    uint64_t pixel_capacity;
    uint64_t required_bytes;
} InkpodSequenceThumbnailBuffer;

/** @brief motion-check の FPS と loop/selection/light-table flags を渡す入力。 */
typedef struct InkpodMotionCheckInput {
    uint32_t struct_size;
    uint32_t fps;
    uint64_t flags;
} InkpodMotionCheckInput;

/** @brief motion-check の current cell と thumbnail metadata の caller-owned 出力。 */
typedef struct InkpodMotionFrame {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t sequence_index;
    uint32_t cell_number;
    uint32_t thumbnail_width;
    uint32_t thumbnail_height;
    uint32_t reserved;
    uint64_t thumbnail_checksum;
} InkpodMotionFrame;

/**
 * @brief library が実装する ABI version を返す。
 * @par 契約
 * 任意スレッド、引数なし、失敗なし。Core/stroke/preview、revision、dirty、Undo に影響しない。
 */
uint32_t inkpod_abi_version(void);

/**
 * @brief single-writer Core を作成する。
 * @par スレッド
 * 呼び出したスレッドが Core owner thread になる。
 * @par NULL・サイズ・所有権
 * `config` と `out_core` は非 NULL・非重複。`config->struct_size` は
 * ABI v1 全体以上、`abi_version` は一致が必要。`config` は呼び出し中だけ borrowed。
 * `*out_core` は事前に NULL。成功時だけ Rust-owned handle を格納する。
 * @par 状態
 * 成功時は文書なし、stroke/preview なしの Core。失敗時 `*out_core` は NULL のまま。
 * revision、dirty、Undo はまだ存在しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INCOMPATIBLE_ABI`、`PANIC`。
 */
InkpodStatus inkpod_core_create(
    const InkpodCoreConfig* config,
    InkpodCore** out_core);

/**
 * @brief Core の Rust 所有権を解放し owner 変数を NULL にする。
 * @par スレッド
 * Core が非 NULL なら作成スレッド限定。
 * @par NULL・所有権
 * `core` 自体は非 NULL。`*core == NULL` は成功 no-op。live handle はこの呼び出しが消費する。
 * @par 状態
 * live stroke/preview/floating は破棄する。成功時 `*core == NULL`。失敗時は handle を消費しない。
 * snapshot は独立所有のため Core より長く生存できる。revision、dirty、Undo を commit しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_destroy(InkpodCore** core);

/**
 * @brief sparse な main-line/color 2-plane CellDocument を新規作成する。
 * @par 契約
 * Core owner thread。`core`、`options`、`out_info` は非 NULL・非重複。
 * options は borrowed で完全な `struct_size`、非 0 UUID、bounded 寸法/DPI が必要。
 * 成功時は旧文書を置換し、stable nonzero ID の新規文書情報をコピーする。history は初期化され、
 * stroke/preview は存在しない。失敗時は旧文書と出力を変えない。active stroke/preview 中は不可。
 * @par revision
 * 成功時の初期 revision/dirty は `out_info` が正本。Undo item は作らない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`INVALID_STATE`、`PANIC`。
 */
InkpodStatus inkpod_core_new_cell(
    InkpodCore* core,
    const InkpodCellCreateOptions* options,
    InkpodDocumentInfo* out_info);

/**
 * @brief Validate dimensions, DPI, frames, topology, depth, and count into one immutable plan.
 * @par Ownership
 * The returned Rust-owned handle is independent of a Core and remains live until
 * `inkpod_cell_creation_plan_release`. Invalid input leaves `*out_plan == NULL`.
 */
InkpodStatus inkpod_cell_creation_plan_create(
    const InkpodCellCreationOptions* options,
    InkpodCellCreationPlan** out_plan);

/** @brief Copy the bounded item count without changing plan or Core state. */
InkpodStatus inkpod_cell_creation_plan_count(
    const InkpodCellCreationPlan* plan,
    uint32_t* out_count);

/**
 * @brief Copy all plan items to caller-owned initialized size-prefixed strided records.
 * Invalid capacity, stride, or any record prefix leaves every output record unchanged
 * and writes zero to `out_written`.
 */
InkpodStatus inkpod_cell_creation_plan_copy(
    const InkpodCellCreationPlan* plan,
    InkpodCellCreationPlanItem* output,
    uint32_t capacity,
    uint64_t stride_bytes,
    uint32_t* out_written);

/** @brief Release a plan, nulling the owner pointer; a repeated null release is a no-op. */
InkpodStatus inkpod_cell_creation_plan_release(InkpodCellCreationPlan** plan);

/**
 * @brief Replace one owner-thread Core document from one immutable plan item.
 * @par Atomicity
 * Invalid index, UUID, topology, allocation failure, or other failure leaves the
 * Core document, revision, history, savepoint, and stable-ID cursor unchanged.
 */
InkpodStatus inkpod_core_new_cell_from_plan(
    InkpodCore* core,
    const InkpodCellCreationPlan* plan,
    uint32_t index,
    uint64_t document_uuid_high,
    uint64_t document_uuid_low,
    InkpodDocumentInfo* out_info);

/**
 * @brief committed document metadata をコピーする。
 * @par 契約
 * Core owner thread。`core` と `out_info` は非 NULL・非重複、出力は完全な `struct_size`。
 * 成功時だけ caller-owned 出力を初期化する。query のため revision、dirty、Undo を変えず、
 * live stroke/preview の transient 内容は document info に commit されない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`NO_DOCUMENT`、`PANIC`。
 */
InkpodStatus inkpod_core_get_document_info(
    InkpodCore* core,
    InkpodDocumentInfo* out_info);

/**
 * @brief Rust-owned immutable built-in editor/new-document defaults を caller-owned record へコピーする。
 * @par 契約
 * Core owner thread。document 作成前にも有効。`core` と `out_defaults` は非 NULL、出力は完全な
 * `struct_size` を持つ。query は Core state、revision、dirty、history を一切変更しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_get_editor_defaults(
    InkpodCore* core,
    InkpodEditorDefaults* out_defaults);

/**
 * @brief current document session の Core-owned EditorState を caller-owned record へコピーする。
 * @par 契約
 * Core owner thread。`core` と `out_state` は非 NULL、出力は完全な `struct_size`。返却値は
 * 呼出後 caller が所有するコピーであり query は Editor/document revision、digest、dirty、history、
 * journal、render content を変更しない。同一 document の view は同じ session state を読む。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`NO_DOCUMENT`、`PANIC`。
 */
InkpodStatus inkpod_core_get_editor_state(
    InkpodCore* core,
    InkpodEditorStateInfo* out_state);

/**
 * @brief expected EditorRevision に対して一つの typed EditorState update を原子的に適用する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL、完全な `struct_size`、呼出中だけ borrowed/caller-owned。
 * semantic no-op は revision/digest/dirty を維持する。実変更は EditorRevision/digest/editor dirty のみ
 * 更新し、document revision、StateId、procedure journal、Undo history、render content は不変。
 * stale、未知 enum/flags、overflow、invalid target、failure は出力と Core state を変更しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`NO_DOCUMENT`、`INVALID_STATE`、`PANIC`。
 */
InkpodStatus inkpod_core_update_editor_state(
    InkpodCore* core,
    const InkpodEditorStateUpdate* update,
    InkpodEditorStateInfo* out_state);

/**
 * Persisted grouped edit targetsをdocument tree順でcaller-owned strided storageへ複写する。
 * capacity 0/NULLはsize query。Rustはstorageを保持せず、active targetは集合へ暗黙追加しない。
 */
InkpodStatus inkpod_core_get_edit_targets(
    InkpodCore* core,
    InkpodEditTarget* targets,
    uint64_t capacity,
    uint64_t target_stride_bytes,
    uint64_t* out_count);

/**
 * @brief 現在の有効な編集対象集合の capability matrix を caller-owned record へ返す。
 *
 * Core owner thread 限定の read-only query で、revision、履歴、ID を変更しない。
 */
InkpodStatus inkpod_core_get_edit_target_capabilities(
    InkpodCore* core,
    InkpodEditTargetCapabilities* output);

/**
 * exact EditorRevisionに対してbounded/unique grouped target setを置換する。
 * spanはcall中だけ借用し、Coreが検証・tree-order正規化したowned setだけを保持する。
 */
InkpodStatus inkpod_core_set_edit_targets(
    InkpodCore* core,
    uint64_t expected_editor_revision,
    const InkpodEditTarget* targets,
    uint64_t target_count,
    uint64_t target_stride_bytes,
    InkpodEditorStateInfo* out_state);

/**
 * grouped target commandを一つのcanonical procedure/transaction/Undo単位として適用する。
 * duplicate/merge出力はcaller-owned strided storageへ返し、capacity不足時は変更前に失敗する。
 */
InkpodStatus inkpod_core_apply_edit_target_command(
    InkpodCore* core,
    const InkpodEditTargetCommand* command,
    InkpodDispatchResult* result,
    InkpodEditTarget* output_targets,
    uint64_t output_capacity,
    uint64_t output_stride_bytes,
    uint64_t* out_output_count);

/**
 * @brief Core-owned EditorStateのexact tool/color/diameter/stable targetでstrokeを開始する。
 * @par 契約
 * Core owner thread。`input`とsample spanは呼出中だけborrowedで、Coreは全値をcopyする。成功時に
 * EditorStateを一度だけcaptureし、append/end中の後続EditorState変更はprocedure引数を変えない。
 * 開始自体はdocument/editor revision、digest、dirty、history、render contentをcommitしない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INCOMPATIBLE_ABI`、`UNSUPPORTED`、`WRONG_THREAD`、
 * `NO_DOCUMENT`、`INVALID_STATE`、`PANIC`。
 */
InkpodStatus inkpod_core_editor_stroke_begin(
    InkpodCore* core,
    const InkpodEditorStrokeInput* input);

/**
 * @brief Core-owned EditorState で primary/secondary view の stroke を開始する。
 * @par 契約
 * Core owner thread。`view_id == 0` は primary view、それ以外は同じ Core が所有する
 * live secondary view でなければならない。device 座標は開始時に選択した view transform
 * で document 座標へ正規化し、append/end まで同じ transform を使用する。`input` と sample
 * span は呼出中だけ borrowed で、Core は必要な値を所有する。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INCOMPATIBLE_ABI`、`UNSUPPORTED`、`WRONG_THREAD`、
 * `NO_DOCUMENT`、`INVALID_STATE`、`PANIC`。
 */
InkpodStatus inkpod_core_editor_stroke_begin_for_view(
    InkpodCore* core,
    uint64_t view_id,
    const InkpodEditorStrokeInput* input);

/**
 * @brief current Core session の category 別 logical resource 使用量をコピーする。
 *
 * Core owner thread。`core` と caller-owned `out_usage` は非 NULL、完全な advertised
 * range、非重複でなければならない。成功/no-op/失敗のいずれでも document、view、
 * history、dirty、savepoint は不変。
 */
InkpodStatus inkpod_core_get_resource_usage(
    InkpodCore* core,
    InkpodResourceUsage* out_usage);
/**
 * @brief 紙 frame と margin を 1 transaction で更新する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL・非重複。入力は完全な `struct_size` で呼び出し中だけ borrowed。
 * 成功時は result を書き、実変更があれば revision を進め dirty/1 Undo 単位にする。失敗時は文書・履歴・出力を変えない。
 * live stroke/preview 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`NO_DOCUMENT`、`INVALID_STATE`、`PANIC`。
 */
InkpodStatus inkpod_core_update_paper_frames(
    InkpodCore* core,
    const InkpodPaperFramesInput* input,
    InkpodDispatchResult* result);

/**
 * @brief 互換 main-line/color active plane を切り替える。
 * @par 契約
 * Core owner thread。`core` は非 NULL borrowed、`plane` は既知値。成功時は EditorState target と
 * EditorRevision/digest/editor dirty だけを更新し、document revision、StateId、Undo、render content は
 * 変えない。semantic no-op と失敗は不変。stroke/preview と競合中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`NO_DOCUMENT`、`INVALID_STATE`、`PANIC`。
 */
InkpodStatus inkpod_core_set_active_plane(
    InkpodCore* core,
    InkpodPlaneKind plane);
/**
 * @brief stable layer/plane ID で active node を切り替える。
 * @par 契約
 * Core owner thread。`core` は非 NULL borrowed、ID は現文書内の整合する組。成功時は EditorState target と
 * EditorRevision/digest/editor dirty だけを更新し、document revision、StateId、Undo、render content は
 * 変えない。semantic no-op と失敗は不変。stroke/preview と競合中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`NO_DOCUMENT`、`INVALID_STATE`、`PANIC`。
 */
InkpodStatus inkpod_core_set_active_node(
    InkpodCore* core,
    uint64_t layer_id,
    uint64_t plane_id);

/**
 * @brief bounded・all-or-nothing の seed/closed-region/extension fill を適用する。
 * @par 契約
 * Core owner thread。`core`、`input`、`result` は非 NULL・非重複。入力と nested inclusion-color span は
 * 呼び出し中だけ borrowed で、各構造体サイズ/stride を検証する。成功時 result を書き、変更時だけ revision、dirty、
 * 1 Undo 単位を進める。no-op は変更数 0。`FILL_OVERFLOW` では漏れ候補だけ result に返し、pixel/revision/dirty/Undo は不変。
 * light-table flags は immutable reference を読むだけ。live stroke/preview 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`FILL_OVERFLOW`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_apply_fill(
    InkpodCore* core,
    const InkpodFillInput* input,
    InkpodFillResult* result);

/**
 * @brief gesture開始時にcaptureしたstable layer/plane targetでfillを実行する。
 * @par 契約
 * pointer/span、transaction、status契約は`inkpod_core_apply_fill`と同じ。layer/planeは同じ
 * document namespaceに属する非0のpairでなければならず、後続EditorState変更でretargetしない。
 */
InkpodStatus inkpod_core_apply_fill_for_editor_target(
    InkpodCore* core,
    uint64_t layer_id,
    uint64_t plane_id,
    const InkpodFillInput* input,
    InkpodFillResult* result);

/**
 * @brief 指定 source の document 座標 pixel を exact-depth 色として読む。
 * @par 契約
 * Core owner thread。`core` と `out_color` は非 NULL・非重複、出力は完全な `struct_size`。
 * 成功時のみ色をコピーする。query のため revision、dirty、Undo と stroke/preview state を変えない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`UNSUPPORTED`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_eyedropper(
    InkpodCore* core,
    InkpodEyedropperSource source,
    uint32_t x,
    uint32_t y,
    InkpodColorValue* out_color);

/**
 * @brief document palette を exact-depth color span で置換する。
 * @par 契約
 * Core owner thread。`core`、`input`、`result` は非 NULL・非重複。span は呼び出し中だけ borrowed、
 * 各 record の `struct_size` と stride が必要。成功時は 1 metadata transaction として revision/dirty/Undo を更新し result を書く。
 * 失敗時不変。live stroke/preview 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_palette_set(
    InkpodCore* core,
    const InkpodColorArray* input,
    InkpodDispatchResult* result);
/**
 * @brief document palette を caller-owned buffer へコピーする。
 * @par 契約
 * Core owner thread。`core` と `buffer` は非 NULL・非重複、buffer は完全な `struct_size`。
 * capacity 0 かつ `colors == NULL` は count query。成功時は complete color record と `color_count` を書く。
 * `BUFFER_TOO_SMALL` でも必要 count を返すが storage は所有しない。revision、dirty、Undo、排他状態は不変。
 * @par 主なステータス
 * `OK`、`BUFFER_TOO_SMALL`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_palette_get(
    InkpodCore* core,
    InkpodColorBuffer* buffer);
/**
 * @brief current composite から deterministic palette を生成して置換する。
 * @par 契約
 * Core owner thread。`core` と `result` は非 NULL、bounds 内の maximum/quantization が必要。
 * 成功時 1 metadata transaction として result/revision/dirty/Undo を更新。失敗時不変。stroke/preview 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_palette_generate(
    InkpodCore* core,
    uint32_t maximum_colors,
    uint32_t quantization_bits,
    InkpodDispatchResult* result);
/** @brief exact-current `.inkpalette` schema 1 を同一directoryのtemporary file経由で保存する。 */
InkpodStatus inkpod_palette_file_save(
    const uint8_t* path_utf8,
    uint64_t path_bytes,
    const InkpodColorArray* input);
/** @brief exact-current `.inkpalette` schema 1 をbounded decodeしてsize-query対応bufferへ返す。 */
InkpodStatus inkpod_palette_file_load(
    const uint8_t* path_utf8,
    uint64_t path_bytes,
    InkpodColorBuffer* buffer);
/** @brief borrowed entry spanをexact-current `.inkchart` schema 1へatomic保存する。 */
InkpodStatus inkpod_color_chart_file_save(
    const uint8_t* path_utf8,
    uint64_t path_bytes,
    const InkpodColorChartEntry* entries,
    uint64_t entry_count,
    uint64_t entry_stride_bytes);
/** @brief `.inkchart` schema 1をimmutable Rust-owned handleへbounded decodeする。 */
InkpodStatus inkpod_color_chart_file_load(
    const uint8_t* path_utf8,
    uint64_t path_bytes,
    InkpodColorChartFile** out_chart);
/** @brief immutable chart の entry count を返す。 */
InkpodStatus inkpod_color_chart_file_count(
    const InkpodColorChartFile* chart,
    uint64_t* out_count);
/** @brief chart entry の color とsize-query対応UTF-8 nameをcaller bufferへコピーする。 */
InkpodStatus inkpod_color_chart_file_get(
    const InkpodColorChartFile* chart,
    uint64_t index,
    InkpodColorValue* out_color,
    uint8_t* name_utf8,
    uint64_t name_capacity,
    uint64_t* out_name_bytes);
/** @brief Rust-owned chart handleをexactly once解放しcaller pointerをNULLにする。 */
InkpodStatus inkpod_color_chart_file_release(InkpodColorChartFile** chart);

/**
 * @brief binary/grayscale main-line の exact-depth base color を設定する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL・非重複、`color` は完全な `struct_size` で borrowed。
 * 成功時は 1 metadata/Undo transaction、失敗時不変。stroke/preview 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`UNSUPPORTED`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_set_main_line_color(
    InkpodCore* core,
    const InkpodColorValue* color,
    InkpodDispatchResult* result);
/**
 * @brief binary/grayscale main-line の base color をコピーする。
 * @par 契約
 * Core owner thread。`core` と完全サイズの `out_color` は非 NULL・非重複。成功時だけ出力を初期化し、
 * revision、dirty、Undo、stroke/preview を変えない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`UNSUPPORTED`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_get_main_line_color(
    InkpodCore* core,
    InkpodColorValue* out_color);

/**
 * @brief 色チェック表示 mode を切り替える。
 * @par 契約
 * Core owner thread。`core` は非 NULL borrowed、mode は既知値。成功時は render/view revision のみ変わり得る。
 * document revision、dirty、Undo、committed pixel は不変。失敗時すべて不変。live stroke/preview と併用しても文書を commit しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_set_color_check(
    InkpodCore* core,
    InkpodColorCheckMode mode);

/**
 * @brief pointer-down から up までの全 sample を 1 transaction で適用する convenience API。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL・非重複。input/sample span は呼び出し中だけ borrowed。
 * 成功時は実 pixel 変更があれば revision/dirty を 1 回更新し 1 Undo 単位、no-op は変更なし。失敗時は atomic に不変。
 * live stroke または filter/dust preview 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_apply_stroke(
    InkpodCore* core,
    const InkpodStrokeInput* input,
    InkpodDispatchResult* result);

/**
 * @brief live stroke を開始し initial sample を transient preview へ適用する。
 * @par 契約
 * Core owner thread。`core`/`input` は非 NULL・非重複、style/sample span は呼び出し中だけ borrowed。
 * 成功時 live stroke が 1 個存在するが committed revision、dirty、Undo は不変。失敗時 session を残さない。
 * 既存 stroke、filter/dust preview、floating 競合中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_stroke_begin(
    InkpodCore* core,
    const InkpodStrokeInput* input);
/**
 * @brief live stroke へ sample batch を順序どおり追加する。
 * @par 契約
 * Core owner thread。`core`/`span` は非 NULL・非重複、span は呼び出し中だけ borrowed で完全サイズ/stride が必要。
 * 成功時 transient preview だけ更新し revision/dirty/Undo は不変。stroke 不在または preview 競合は `INVALID_STATE`。
 * append 失敗時は部分 stroke を後で commit できないよう session を無効化する。出力はない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_stroke_append(
    InkpodCore* core,
    const InkpodStrokeSampleSpan* span);
/**
 * @brief live stroke を終了し、全 sample を高々 1 transaction で commit する。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL・非重複、result は完全サイズ。
 * 成功時、実変更なら revision/dirty と 1 Undo 単位を進め、no-op なら進めない。どちらも session を終了する。
 * stroke 不在は `INVALID_STATE`。失敗時 committed base は不変で session は commit されない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_stroke_end(
    InkpodCore* core,
    InkpodDispatchResult* result);
/**
 * @brief live stroke を破棄して committed base へ戻す。
 * @par 契約
 * Core owner thread。`core` は非 NULL borrowed。stroke 不在は成功 no-op。
 * 成功時 session を消すが revision、dirty、Undo は begin 前から不変。filter preview state は変更しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_stroke_cancel(InkpodCore* core);

/**
 * @brief history cursor を 1 transaction 戻す。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL・非重複。成功時 result と document revision を更新し、
 * dirty は savepoint との位置関係で再計算する。Undo item を新規作成せず Redo を可能にする。失敗/no-op 時は出力未使用・不変。
 * stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_core_undo(
    InkpodCore* core,
    InkpodDispatchResult* result);
/**
 * @brief history cursor を 1 transaction 進める。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL・非重複。成功時 result/revision を更新し dirty を savepoint から再計算。
 * history item は増やさない。失敗/no-op 時不変。stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_core_redo(
    InkpodCore* core,
    InkpodDispatchResult* result);
/**
 * @brief history cursor と item count をコピーする。
 * @par 契約
 * Core owner thread。`core`/`out_info` は非 NULL・非重複、出力は完全サイズ。成功時だけ書く。
 * query のため revision、dirty、Undo、stroke/preview state は不変。
 * @par 主なステータス
 * `OK`、`NO_DOCUMENT`、`WRONG_THREAD`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_core_history_info(
    InkpodCore* core,
    InkpodHistoryInfo* out_info);
/**
 * @brief 指定 history item の適用状態と UTF-8 名を caller buffer へコピーする。
 * @par 契約
 * Core owner thread。`core`/`out_item` は非 NULL・非重複、完全サイズ。name capacity 0/NULL は size query。
 * 成功時 complete record、`BUFFER_TOO_SMALL` 時も必要 `name_bytes` を返す。revision、dirty、Undo、排他状態は不変。
 * @par 主なステータス
 * `OK`、`BUFFER_TOO_SMALL`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_history_item(
    InkpodCore* core,
    uint64_t index,
    InkpodHistoryItem* out_item);
/**
 * @brief history cursor を指定位置へ atomic に移動する。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL、target は 0..item_count。成功時 revision/result と dirty を更新するが
 * item は追加しない。失敗時不変。stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_history_jump(
    InkpodCore* core,
    uint64_t target_cursor,
    InkpodDispatchResult* result);
/**
 * @brief active selection に関係する直前状態へ transaction として戻す。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL。成功時実変更があれば revision/dirty/1 Undo 単位を追加。
 * 失敗時不変。stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_core_revert_active_selection(
    InkpodCore* core,
    InkpodDispatchResult* result);

/**
 * @brief Reads native format, checkpoint strategy, journal work, and asset counters.
 * @par Contract
 * Owner-thread query. Success writes a complete `InkpodPersistenceInfo`; failure
 * leaves it unchanged. The call never performs replay or changes revisions,
 * savepoints, history, dirty state, or caches.
 */
InkpodStatus inkpod_core_get_persistence_info(
    InkpodCore* core,
    InkpodPersistenceInfo* out_info);

/**
 * @brief Builds the exact confirmation token for an explicit compacted copy.
 * @par Contract
 * The caller must present both history counts before confirmation. The token is
 * invalidated by any document, editor, or journal change. This query has no side effects.
 */
InkpodStatus inkpod_core_compaction_plan(
    InkpodCore* core,
    InkpodCompactionPlan* out_plan);

/**
 * @brief Writes a separate current-version file with current state as new Genesis.
 * @par Contract
 * `plan` must exactly match the current confirmation token. Success intentionally
 * omits prior Undo/Redo and inactive branches, but never changes or rebinds the live
 * Core, current path, revisions, savepoints, dirty state, or history.
 */
InkpodStatus inkpod_core_write_compacted_copy(
    InkpodCore* core,
    const uint8_t* path_utf8,
    uint64_t path_bytes,
    const InkpodCompactionPlan* plan);

/**
 * @brief current document を `.inkpod` へ atomic に通常保存する。
 * @par 契約
 * Core owner thread。`core`/path/`out_info` は非 NULL・非重複、path は非空 UTF-8 byte span として呼び出し中だけ borrowed。
 * 同一 directory の一時 file を完成・flush・close 後に置換する。成功時 revision/Undo は不変、
 * document/editor normal savepointとpathを、durable replacement成功後だけ更新してout_infoを書く。
 * Genesis、Assets、procedure/history journal、EditorStateを一つのcurrent native containerへ保存する。
 * 失敗時既存file・文書・両savepoint・出力は不変。
 * stroke/preview/floating 中も `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`IO_ERROR`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_save(
    InkpodCore* core,
    const uint8_t* path_utf8,
    uint64_t path_bytes,
    InkpodDocumentInfo* out_info);
/**
 * @brief `.inkpod` を検証・decode して current document を置換する。
 * @par 契約
 * Core owner thread。`core`/非空 UTF-8 path/`out_info` は非 NULL・非重複。path は呼び出し中だけ borrowed。
 * 成功時のみ文書と history/savepoint を置換し out_info を書く。open 自体は Undo item を作らず、通常 dirty は解消済み。
 * decode/IO 失敗時は旧文書・revision・dirty・Undo・出力を完全に保つ。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`IO_ERROR`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_open(
    InkpodCore* core,
    const uint8_t* path_utf8,
    uint64_t path_bytes,
    InkpodDocumentInfo* out_info);
/**
 * @brief current document を recovery 用 `.inkpod` へ atomic 保存する。
 * @par 契約
 * Core owner thread。通常 save と同じ非 NULL/UTF-8/borrowed path・出力サイズ規則。
 * 成功時 out_info を書くが document revision、dirty、Undo、normal path/savepoint は変えない。Genesis、Assets、
 * journal、EditorStateを復元可能な回復containerへ書く。失敗時文書と既存出力 file は不変。stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`IO_ERROR`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_autosave(
    InkpodCore* core,
    const uint8_t* path_utf8,
    uint64_t path_bytes,
    InkpodDocumentInfo* out_info);
/**
 * @brief recovery container を dirty・pathless document として開く。
 * @par 契約
 * Core owner thread。`core`/非空 UTF-8 path/`out_info` は非 NULL・非重複、path は一時 borrowed。
 * 成功時のみ current document/history を置換し `RECOVERED|DIRTY` を含む info を書く。normal path/savepoint は引き継がず Undo item は作らない。
 * 失敗時旧文書と出力は不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`IO_ERROR`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_open_recovery(
    InkpodCore* core,
    const uint8_t* path_utf8,
    uint64_t path_bytes,
    InkpodDocumentInfo* out_info);
/**
 * @brief current normal path の保存済み内容を再読込して未保存変更を破棄する。
 * @par 契約
 * Core owner thread。`core`/`out_info` は非 NULL・非重複、出力は完全サイズ。成功時のみ文書/history を置換し dirty を解消、
 * Undo history を新規開始する。失敗時現文書を保つ。normal path がない場合や stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`NO_DOCUMENT`、`INVALID_STATE`、`IO_ERROR`、`WRONG_THREAD`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_core_revert(
    InkpodCore* core,
    InkpodDocumentInfo* out_info);

/**
 * @brief primary view に pan/zoom/fit/1:1/viewport/flip/overlay command を適用する。
 * @par 値
 * PAN(dx,dy)、ZOOM(factor,anchor_x,anchor_y)、FIT/ONE_TO_ONE/VIEWPORT_RESIZED(w,h)、
 * BOX_ZOOM(document_x,document_y,document_w,document_h)、SET_*(enabled)。flip は値を無視する。
 * BOX_ZOOM 以外は Canvas client device pixel。resize は manual view を保持し persistent Fit/1:1 だけ再計算する。
 * @par 契約
 * Core owner thread。`core`/`input`/`out_info` は非 NULL・非重複、構造体は完全サイズ。
 * 成功時 view revision/info のみ更新し document revision、dirty、Undo は不変。失敗時不変。live stroke と競合する view mutation は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_apply_view(
    InkpodCore* core,
    const InkpodViewInput* input,
    InkpodDocumentInfo* out_info);

/**
 * @brief layer へ plane kind/storage format を追加できるか読み取り専用で検証する。
 * @par 契約
 * Core owner thread。`core` は非 NULL。成功・失敗とも document、stable ID、revision、dirty、history は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_validate_plane_creation(
    InkpodCore* core,
    uint64_t layer_id,
    InkpodTypedPlaneKind kind,
    InkpodStoragePixelFormat pixel_format);
/**
 * @brief typed layer/plane tree を作成・複製・削除・並替・変換・統合する。
 * @par 契約
 * Core owner thread。`core`/`input`/`result`/`out_object_id` は非 NULL・非重複。
 * input と UTF-8 名は呼び出し中だけ borrowed。成功時 result と stable ID を書き、実変更を 1 revision/dirty/Undo 単位で commit。
 * 失敗時 document/history/両出力は不変。stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`UNSUPPORTED`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_tree_edit(
    InkpodCore* core,
    const InkpodTreeEdit* input,
    InkpodDispatchResult* result,
    uint64_t* out_object_id);
/**
 * @brief index 指定の layer または plane metadata/name をコピーする。
 * @par 契約
 * Core owner thread。`core`/`out_info` は非 NULL・非重複、出力は完全サイズ。
 * `plane_index == UINT32_MAX` は layer 自体。name capacity 0/NULL は size query、`BUFFER_TOO_SMALL` でも必要 `name_bytes` を返す。
 * query のため revision、dirty、Undo、排他状態は不変。
 * @par 主なステータス
 * `OK`、`BUFFER_TOO_SMALL`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_node_get(
    InkpodCore* core,
    uint32_t layer_index,
    uint32_t plane_index,
    InkpodNodeInfo* out_info);
/**
 * @brief stable layer ID の内容だけを縮小合成し straight RGBA8 でコピーする。
 * @par 契約
 * Core owner thread。`core`/`output` は非 NULL・非重複、構造体は完全サイズ。
 * capacity 0/NULL は size query。layer 自体が非表示でも内容を表示し、plane visibility と
 * layer/plane opacity は反映する。成功・失敗とも selection、revision、dirty、Undo は不変。
 * @par 主なステータス
 * `OK`、`BUFFER_TOO_SMALL`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`UNSUPPORTED`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_layer_thumbnail(
    InkpodCore* core,
    InkpodLayerThumbnailBuffer* output);
/**
 * @brief shape/points/wand 条件から selection mask を new/add/subtract/intersect する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL・非重複。input/point span は完全サイズ/stride で呼び出し中だけ borrowed。
 * 成功時実変更を 1 revision/dirty/Undo 単位で commit し result を書く。失敗時不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_apply_selection(
    InkpodCore* core,
    const InkpodSelectionInput* input,
    InkpodDispatchResult* result);

/**
 * @brief gesture開始時にcaptureしたstable layer/plane targetでselectionを実行する。
 * @par 契約
 * pointer/span、transaction、status契約は`inkpod_core_apply_selection`と同じ。layer/planeは同じ
 * document namespaceに属する非0のpairでなければならず、後続EditorState変更でretargetしない。
 */
InkpodStatus inkpod_core_apply_selection_for_editor_target(
    InkpodCore* core,
    uint64_t layer_id,
    uint64_t plane_id,
    const InkpodSelectionInput* input,
    InkpodDispatchResult* result);
/**
 * @brief Evaluates scoped exact color replacement without mutating Core state.
 * @par Contract
 * Core owner thread; all records and advertised point spans are borrowed and
 * must be complete, aligned, and non-overlapping. Stale revision, invalid mode,
 * hidden/locked target, invalid span, or bounded-work overflow changes nothing.
 */
InkpodStatus inkpod_core_preview_scoped_color_replace(
    InkpodCore* core,
    const InkpodScopedColorReplaceInput* input,
    InkpodScopedColorReplacePreview* output);
/**
 * @brief Commits one scoped exact replacement as one canonical Undo unit.
 * @par Contract
 * Ownership and validation match the preview API. Success writes `result`;
 * semantic no-op preserves revision/history. Failure and stale input are atomic.
 */
InkpodStatus inkpod_core_apply_scoped_color_replace(
    InkpodCore* core,
    const InkpodScopedColorReplaceInput* input,
    InkpodDispatchResult* result);
/**
 * @brief call開始時にcurrent EditorStateからcaptureしたactive planeの指定色と同じ／異なるpixelをselectionへ合成する。
 * @par 契約
 * Core owner thread。`core`/`color`/`result` は非 NULL・非重複、color は完全サイズで borrowed。
 * 成功時実変更を 1 revision/dirty/Undo 単位で commit。失敗時不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_select_color(
    InkpodCore* core,
    const InkpodColorValue* color,
    uint16_t tolerance,
    uint32_t different,
    InkpodSelectionOperation operation,
    InkpodDispatchResult* result);
/**
 * @brief command開始時にcaptureしたstable layer/plane targetから色選択する。
 * @par 契約
 * pointer、transaction、status契約は`inkpod_core_select_color`と同じ。layer/planeは同じ
 * document namespaceに属する非0のpairでなければならず、後続EditorState変更でretargetしない。
 */
InkpodStatus inkpod_core_select_color_for_editor_target(
    InkpodCore* core,
    uint64_t layer_id,
    uint64_t plane_id,
    const InkpodColorValue* color,
    uint16_t tolerance,
    uint32_t different,
    InkpodSelectionOperation operation,
    InkpodDispatchResult* result);
/**
 * @brief selection mask を invert/expand/shrink する。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL、operation と pixel 上限を検証。
 * 成功時実変更を 1 revision/dirty/Undo 単位で commit。失敗時不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_selection_adjust(
    InkpodCore* core,
    uint32_t operation,
    uint32_t pixels,
    InkpodDispatchResult* result);
/**
 * @brief document selection mask を空にする。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL。成功時、非空なら 1 revision/dirty/Undo 単位、既に空なら no-op。
 * 失敗時不変。stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_core_selection_clear(
    InkpodCore* core,
    InkpodDispatchResult* result);
/**
 * @brief selection mask から新しい selection layer を作る。
 * @par 契約
 * Core owner thread。`core`/非空 UTF-8 name/`result`/`out_layer_id` は非 NULL・非重複。name は呼び出し中だけ borrowed。
 * 成功時 stable ID と result を書き 1 revision/dirty/Undo 単位。失敗時出力/文書不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_selection_to_layer(
    InkpodCore* core,
    const uint8_t* name_utf8,
    uint64_t name_bytes,
    InkpodDispatchResult* result,
    uint64_t* out_layer_id);
/**
 * @brief selection layer を document selection へ replace/add/subtract する。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL、layer ID と operation は有効値。
 * 成功時実変更を 1 revision/dirty/Undo 単位で commit。失敗時不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_selection_from_layer(
    InkpodCore* core,
    uint64_t layer_id,
    uint32_t operation,
    InkpodDispatchResult* result);

/**
 * @brief current selection/plane を typed clipboard payload へコピーする。
 * @par 契約
 * Core owner thread。`core`/`out_clipboard` は非 NULL・非重複、`*out_clipboard == NULL` が必要。
 * 成功時だけ Rust-owned immutable handle を格納する。document revision、dirty、Undo は変えない。
 * 失敗時 owner は NULL のまま。live stroke/preview と競合中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_clipboard_copy(
    InkpodCore* core,
    InkpodClipboard** out_clipboard);
/**
 * @brief clipboard handle を解放して owner 変数を NULL にする。
 * @par 契約
 * 任意スレッド。`clipboard` 自体は非 NULL、`*clipboard == NULL` は成功 no-op。
 * 成功時所有権を消費し別名を無効化。Core/revision/dirty/Undo/stroke/preview に影響しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_clipboard_release(InkpodClipboard** clipboard);
/**
 * @brief clipboard を互換 destination へ floating paste として開始する。
 * @par 契約
 * Core owner thread。`core`/`clipboard` は非 NULL・非重複。clipboard は呼び出し中 borrowed で payload を Core が複製するため、
 * 成功後は元 handle を解放できる。成功時 floating state だけ作り committed revision/dirty/Undo は不変。
 * stroke/preview/既存 floating 中は `INVALID_STATE`。失敗時 state 不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`UNSUPPORTED`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_paste_begin(
    InkpodCore* core,
    const InkpodClipboard* clipboard);
/**
 * @brief destination/変換 mode を明示して floating paste を開始する。
 * @par 契約
 * `inkpod_core_paste_begin` と同じ。mode は既知値、clipboard は一時 borrowed。
 * 成功時 preview state のみで revision/dirty/Undo は不変。競合 stroke/preview/floating は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`UNSUPPORTED`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_paste_begin_mode(
    InkpodCore* core,
    const InkpodClipboard* clipboard,
    uint32_t mode);
/**
 * @brief create-plane targetを保持し、floating commit時だけplane作成とpasteを一括commitする。
 * @par 契約 `target`は`INKPOD_TREE_CREATE_PLANE`、visible+editable、完全なtyped/name入力。
 * begin/cancelはdocument不変、commit成功はplane作成とpixel適用を一つのUndo単位にする。
 */
InkpodStatus inkpod_core_paste_begin_new_plane(
    InkpodCore* core,
    const InkpodClipboard* clipboard,
    const InkpodTreeEdit* target);
/**
 * @brief typed clipboard を straight RGBA8 caller buffer へ rasterize する。
 * @par 契約
 * 任意スレッド。`clipboard`/`output` は非 NULL・非重複、output は完全サイズ。
 * capacity 0/NULL は size query。成功時 raster metadata/bytes、`BUFFER_TOO_SMALL` 時も `required_bytes` を返す。
 * clipboard の所有権と Core/revision/dirty/Undo/排他状態は不変。
 * @par 主なステータス
 * `OK`、`BUFFER_TOO_SMALL`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_clipboard_render_rgba8(
    const InkpodClipboard* clipboard,
    InkpodClipboardRasterBuffer* output);
/**
 * @brief external straight RGBA8 raster から typed clipboard handle を作る。
 * @par 契約
 * 任意スレッド。`input`/`out_clipboard` は非 NULL・非重複、入力は完全サイズで呼び出し中だけ borrowed、
 * `*out_clipboard == NULL`。成功時 raster をコピーした Rust-owned handle を格納。失敗時 NULL のまま。
 * Core/revision/dirty/Undo/排他状態に影響しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_clipboard_create_rgba8(
    const InkpodClipboardRgbaInput* input,
    InkpodClipboard** out_clipboard);
/**
 * @brief active floating paste の translate/scale/rotate preview を置換する。
 * @par 契約
 * Core owner thread。`core`/`input` は非 NULL・非重複、input は完全サイズ borrowed、finite 値が必要。
 * 成功時 transient preview のみ更新し committed revision/dirty/Undo は不変。floating 不在または stroke/preview 中は `INVALID_STATE`。
 * 失敗時元 preview を保つ。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_floating_transform(
    InkpodCore* core,
    const InkpodFloatingTransform* input);
/**
 * @brief active floating paste を高々 1 transaction で commit する。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL。成功時実変更なら revision/dirty/1 Undo 単位を進め floating state を終了。
 * floating 不在または stroke/preview 中は `INVALID_STATE`。失敗時 committed base と state を保つ。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_floating_commit(
    InkpodCore* core,
    InkpodDispatchResult* result);
/**
 * @brief active floating paste を破棄して committed base へ戻す。
 * @par 契約
 * Core owner thread。`core` は非 NULL。floating 不在は成功 no-op。
 * 成功時 revision、dirty、Undo は begin 前から不変。stroke/filter preview は変更しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_floating_cancel(InkpodCore* core);
/**
 * @brief active selection 内の selected content を消去する。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL。成功時実変更を 1 revision/dirty/Undo 単位で commit。
 * 失敗時不変。stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_core_clear_selected_content(
    InkpodCore* core,
    InkpodDispatchResult* result);

/**
 * @brief document content と metadata を水平／垂直反転する。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL、axis は既知値。成功時 1 revision/dirty/Undo 単位、失敗時不変。
 * view-only flip とは異なる。stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_mirror_document(
    InkpodCore* core,
    uint32_t axis,
    InkpodDispatchResult* result);
/**
 * @brief document content と metadata を左／右 90 度回転する。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL、direction は既知値。成功時 1 revision/dirty/Undo 単位、失敗時不変。
 * stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_rotate_document(
    InkpodCore* core,
    uint32_t direction,
    InkpodDispatchResult* result);
/**
 * @brief document 寸法/DPI を anchor と resample 方針に従い変更する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL・非重複、input/result は完全サイズ。
 * 成功時全 content/metadata を atomic に 1 revision/dirty/Undo 単位で commit。失敗時 partial resize は残さない。
 * stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_resize_document(
    InkpodCore* core,
    const InkpodDocumentResizeInput* input,
    InkpodDispatchResult* result);
/**
 * @brief document guide を追加する。
 * @par 契約
 * Core owner thread。`core`/`result`/`out_guide_id` は非 NULL・非重複、axis は既知値。
 * 成功時 stable ID/result を書き 1 revision/dirty/Undo 単位。失敗時出力/文書不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_guide_add(
    InkpodCore* core,
    uint32_t axis,
    int32_t position,
    InkpodDispatchResult* result,
    uint64_t* out_guide_id);
/**
 * @brief stable guide ID の位置を移動する。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL、ID は現文書内。成功時実変更を 1 revision/dirty/Undo 単位。
 * 失敗時不変。stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_guide_move(
    InkpodCore* core,
    uint64_t guide_id,
    int32_t position,
    InkpodDispatchResult* result);
/**
 * @brief stable guide ID を削除する。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL。成功時 1 revision/dirty/Undo 単位、失敗時不変。
 * stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_guide_delete(
    InkpodCore* core,
    uint64_t guide_id,
    InkpodDispatchResult* result);
/** @brief 全 document guide を一つの canonical primitive として削除する。 */
InkpodStatus inkpod_core_guide_delete_all(
    InkpodCore* core,
    InkpodDispatchResult* result);
/**
 * @brief document grid 定義を置換する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL・非重複、input/result は完全サイズ、spacing/subdivision は bounded。
 * 成功時実変更を 1 revision/dirty/Undo 単位。失敗時不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_grid_set(
    InkpodCore* core,
    const InkpodGridInput* input,
    InkpodDispatchResult* result);
/**
 * @brief device point を view transform で document 座標へ変換し selection/color を読む。
 * @par 契約
 * Core owner thread。`core`/完全サイズの `out_locator` は非 NULL、device 値は finite、`view_id == 0` は primary。
 * 成功時だけ caller-owned 出力を初期化。query のため revision、dirty、Undo、排他状態は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_locator_sample(
    InkpodCore* core,
    uint64_t view_id,
    double device_x,
    double device_y,
    InkpodLocatorOutput* out_locator);
/**
 * @brief device 点を中心とする bounded composite-color neighborhood を一括取得する。
 * @par 契約
 * Core owner thread。`radius` は 0..16。capacity 0/NULL は size query。範囲外 pixel は
 * transparent RGBA8。成功時 metadata/bytes、`BUFFER_TOO_SMALL` 時も required bytes と
 * dimensions を返す。query のため document/view revision、dirty、Undo は不変。
 * @par 主なステータス
 * `OK`、`BUFFER_TOO_SMALL`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`UNSUPPORTED`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_locator_neighborhood(
    InkpodCore* core,
    uint64_t view_id,
    double device_x,
    double device_y,
    InkpodLocatorNeighborhoodBuffer* output);
/**
 * @brief command の shortcut key chord を設定し、既存 conflict を置換する。
 * @par 契約
 * Core owner thread。`core` は非 NULL、command/key/modifier は bounded 既知値。成功時 application 設定だけ変更し、
 * document revision、dirty、Undo、stroke/preview は不変。失敗時 shortcut map 不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_shortcut_rebind(
    InkpodCore* core,
    uint32_t command_id,
    uint32_t virtual_key,
    uint32_t modifiers);
/**
 * @brief normalized key chord を command ID へ解決する。
 * @par 契約
 * Core owner thread。`core`/`out_command_id` は非 NULL。成功時だけ ID を書く。
 * query のため document revision、dirty、Undo、stroke/preview は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_shortcut_resolve(
    InkpodCore* core,
    uint32_t virtual_key,
    uint32_t modifiers,
    uint32_t* out_command_id);
/**
 * @brief shortcut map を deterministic default へ戻す。
 * @par 契約
 * Core owner thread。`core` は非 NULL。成功時 application 設定だけ変更し、document revision、dirty、Undo、
 * stroke/preview は不変。失敗時 map 不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_shortcut_reset(InkpodCore* core);
/**
 * @brief application提供の完全な既定shortcut集合を検証して登録し、現在値にも反映する。
 * @par 契約
 * Core owner thread。各列は1～4 stroke、commandは一意、全列はprefix-free。入力は呼出中だけ借用する。
 * 成功時だけ既定値と現在値を同時置換し、document revision、dirty、Undoは不変。
 */
InkpodStatus inkpod_core_shortcut_defaults_set(
    InkpodCore* core,
    const InkpodShortcutSequence* sequences,
    uint64_t sequence_count,
    uint64_t sequence_stride_bytes);
/** @brief 現在の完全なshortcut集合をtransactionalに置換する。 */
InkpodStatus inkpod_core_shortcut_sequences_set(
    InkpodCore* core,
    const InkpodShortcutSequence* sequences,
    uint64_t sequence_count,
    uint64_t sequence_stride_bytes);
/** @brief 現在のshortcut集合をcaller bufferへコピーする。buffer無しは必要件数queryになる。 */
InkpodStatus inkpod_core_shortcut_sequences_copy(
    InkpodCore* core,
    InkpodShortcutSequence* out_sequences,
    uint64_t sequence_capacity,
    uint64_t sequence_stride_bytes,
    uint64_t* out_sequence_count);
/**
 * @brief caller-owned immutable shortcut表で入力列を解決するthread-independent pure helper。
 * @par 契約
 * Core handle不要。入力表は Core で検証・copyした完全サイズのprefix-free表とする。
 * 各recordの形式不正は拒否する。成功時matchとcommandを書き、
 * prefix/noneではcommandを0にする。
 */
InkpodStatus inkpod_shortcut_sequence_resolve(
    const InkpodShortcutSequence* sequences,
    uint64_t sequence_count,
    uint64_t sequence_stride_bytes,
    const InkpodShortcutStroke* strokes,
    uint32_t stroke_count,
    InkpodShortcutMatch* out_match,
    uint32_t* out_command_id);
/**
 * @brief primary とは独立した logical view を作成する。
 * @par 契約
 * Core owner thread。`core`/`out_view_id` は非 NULL・非重複。成功時 stable nonzero view ID を書く。
 * document revision、dirty、Undo は不変。失敗時出力不変。文書未作成や競合 state は拒否する。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_view_create(
    InkpodCore* core,
    uint64_t* out_view_id);
/**
 * @brief secondary logical view へ view command を適用する。
 * @par 契約
 * Core owner thread。`core`/完全サイズの borrowed `input` は非 NULL、view ID は有効。
 * 成功時その view revision/state のみ更新し document revision、dirty、Undo は不変。失敗時不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_view_apply(
    InkpodCore* core,
    uint64_t view_id,
    const InkpodViewInput* input);
/**
 * @brief secondary logical view を閉じる。
 * @par 契約
 * Core owner thread。`core` は非 NULL、view ID は secondary view。成功時 logical state だけ解放する。
 * 既存 snapshot は独立 owned のため有効。document revision、dirty、Undo は不変。失敗時不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_view_close(
    InkpodCore* core,
    uint64_t view_id);

/**
 * @brief PNG/TIFF/TGA/BMP bytes を decode して current document を置換する。
 * @par 契約
 * Core owner thread。`core`/非空 bytes/`out_info` は非 NULL・非重複、bytes は呼び出し中だけ borrowed、UUID は非 0。
 * 成功時のみ文書/history を置換し info を書く。import 自体は Undo item を作らない。失敗時旧文書・revision・dirty・Undo・出力は不変。
 * stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`UNSUPPORTED`、`IO_ERROR`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_import_common_raster(
    InkpodCore* core,
    InkpodCommonRasterFormat format,
    const uint8_t* bytes,
    uint64_t byte_count,
    uint64_t document_uuid_high,
    uint64_t document_uuid_low,
    InkpodDocumentInfo* out_info);
/**
 * @brief current document を PNG/TIFF/TGA/BMP bytes へ encode する。
 * @par 契約
 * Core owner thread。`core`/`out_buffer` は非 NULL・非重複、`*out_buffer == NULL`、format/white flag は既知値。
 * 成功時 Rust-owned immutable byte buffer を格納。失敗時 NULL のまま。query/export のため revision、dirty、Undo、排他 state は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`UNSUPPORTED`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_export_common_raster(
    InkpodCore* core,
    InkpodCommonRasterFormat format,
    uint32_t composite_white,
    InkpodByteBuffer** out_buffer);
/**
 * @brief immutable byte buffer の borrowed byte span を取得する。
 * @par 契約
 * 任意スレッド。`buffer`/`out_bytes`/`out_byte_count` は非 NULL・非重複。同じ handle の release と外部同期する。
 * 成功時 span/count を書き、span は buffer release まで有効。失敗時出力未使用。Core/revision/dirty/Undo は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_byte_buffer_view(
    const InkpodByteBuffer* buffer,
    const uint8_t** out_bytes,
    uint64_t* out_byte_count);
/**
 * @brief byte buffer handle を解放して owner 変数を NULL にする。
 * @par 契約
 * 任意スレッド。owner pointer は非 NULL、`*buffer == NULL` は成功 no-op。成功後 borrowed byte span は無効。
 * Core/revision/dirty/Undo/排他状態に影響しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_byte_buffer_release(InkpodByteBuffer** buffer);

/**
 * @brief copied raster source から light-table item を追加する。
 * @par 契約
 * Core owner thread。4 pointer は非 NULL・非重複。input/name/raster bytes は完全サイズで呼び出し中だけ borrowed。
 * 成功時 source をコピーし stable ID/result を書き 1 revision/dirty/Undo 単位。失敗時出力/文書不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_light_table_add_item(
    InkpodCore* core,
    const InkpodLightTableItemInput* input,
    InkpodDispatchResult* result,
    uint64_t* out_item_id);
/**
 * @brief light-table set/item を create/duplicate/delete/rename/reorder/update する。
 * @par 契約
 * Core owner thread。4 pointer は非 NULL・非重複、input/name は呼び出し中だけ borrowed。
 * 成功時 result と対象/新規 stable ID を書き、実変更を 1 revision/dirty/Undo 単位。失敗時不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_light_table_edit(
    InkpodCore* core,
    const InkpodLightTableEdit* input,
    InkpodDispatchResult* result,
    uint64_t* out_object_id);
/**
 * @brief index 指定 light-table set の metadata/name をコピーする。
 * @par 契約
 * Core owner thread。`core`/完全サイズの `output` は非 NULL。name capacity 0/NULL は size query。
 * `BUFFER_TOO_SMALL` でも必要 `name_bytes` を返す。revision、dirty、Undo、排他状態は不変。
 * @par 主なステータス
 * `OK`、`BUFFER_TOO_SMALL`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_light_table_set_get(
    InkpodCore* core,
    uint32_t index,
    InkpodLightTableSetInfo* output);
/**
 * @brief active set の index 指定 item metadata/name をコピーする。
 * @par 契約
 * Core owner thread。`core`/完全サイズの `output` は非 NULL。name buffer は caller-owned size-query 対応。
 * query のため revision、dirty、Undo、排他状態は不変。
 * @par 主なステータス
 * `OK`、`BUFFER_TOO_SMALL`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_light_table_item_get(
    InkpodCore* core,
    uint32_t index,
    InkpodLightTableItemInfo* output);
/**
 * @brief encoded common raster を decode して light-table item に追加する。
 * @par 契約
 * Core owner thread。`core`/bytes/name/`result`/`out_item_id` は非 NULL・非重複、byte/name span は呼び出し中だけ borrowed。
 * 成功時 decode source をコピーし 1 revision/dirty/Undo 単位、ID/result を書く。失敗時不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`UNSUPPORTED`、`IO_ERROR`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_light_table_add_common_raster(
    InkpodCore* core,
    InkpodCommonRasterFormat format,
    const uint8_t* bytes,
    uint64_t byte_count,
    const uint8_t* name_utf8,
    uint64_t name_bytes,
    uint64_t document_uuid_high,
    uint64_t document_uuid_low,
    uint64_t source_revision,
    InkpodDispatchResult* result,
    uint64_t* out_item_id);
/**
 * @brief 既存 light-table item の source を encoded raster で再読込する。
 * @par 契約
 * Core owner thread。`core`/非空 bytes/`result` は非 NULL、bytes は一時 borrowed、item ID/UUID/revision は検証する。
 * 成功時 source だけを atomic に置換し 1 revision/dirty/Undo 単位。失敗時旧 source/文書不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`UNSUPPORTED`、`IO_ERROR`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_light_table_reload_common_raster(
    InkpodCore* core,
    uint64_t item_id,
    InkpodCommonRasterFormat format,
    const uint8_t* bytes,
    uint64_t byte_count,
    uint64_t document_uuid_high,
    uint64_t document_uuid_low,
    uint64_t source_revision,
    InkpodDispatchResult* result);
/**
 * @brief active light-table set の global opacity を設定する。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL、opacity は 0..1000。成功時実変更を 1 revision/dirty/Undo 単位。
 * 失敗時不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_light_table_set_global_opacity(
    InkpodCore* core,
    uint32_t opacity_milli,
    InkpodDispatchResult* result);
/**
 * @brief light-table composite の document pixel を exact-depth 色で読む。
 * @par 契約
 * Core owner thread。`core`/完全サイズの `out_color` は非 NULL。成功時だけ出力を初期化。
 * query のため revision、dirty、Undo、stroke/preview state は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`UNSUPPORTED`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_light_table_sample(
    InkpodCore* core,
    uint32_t x,
    uint32_t y,
    InkpodColorValue* out_color);
/**
 * @brief current clean cell と指定 light-table source を交換する。
 * @par 契約
 * Core owner thread。`core`/完全サイズの `out_info` は非 NULL。current document が dirty なら
 * `UNSAVED_CHANGES` を返し、文書/revision/出力を変えない。成功時 source/current を置換し info を書く。
 * switch 自体は Undo item を作らず、new current の revision/dirty は info が正本。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`UNSAVED_CHANGES`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_light_table_swap(
    InkpodCore* core,
    uint64_t item_id,
    InkpodDocumentInfo* out_info);
/**
 * @brief copied raster cell span で logical sequence を置換する。
 * @par 契約
 * Core owner thread。`core`/`input` は非 NULL・非重複。全 nested struct/name/raster/span は完全サイズ/stride で一時 borrowed。
 * 成功時 Core が全値をコピーし自然順に保持する。current document revision、dirty、Undo は変えない。
 * 失敗時旧 sequence 不変。stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_sequence_set(
    InkpodCore* core,
    const InkpodSequenceInput* input);
/**
 * @brief encoded named-file span を decode して sequence を置換する。
 * @par 契約
 * Core owner thread。`core`/`files` は非 NULL、count/stride/nested sizes と各 name/byte span を検証し呼び出し中だけ borrowed。
 * 成功時全 decode 結果をコピー。current document revision、dirty、Undo は不変。失敗時旧 sequence 不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`UNSUPPORTED`、`IO_ERROR`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_sequence_import_encoded(
    InkpodCore* core,
    InkpodCommonRasterFormat format,
    const InkpodNamedBytesInput* files,
    uint64_t file_count,
    uint64_t file_stride_bytes);
/**
 * @brief file ごとの common-raster format で encoded sequence を decode して置換する。
 * @par 契約
 * Core owner thread。`core`/`files` は非 NULL、count/stride/nested sizes と各
 * format/name/byte span を検証し、呼び出し中だけ borrowed。成功時は全 decode
 * 結果を一括 install し、current document revision、dirty、Undo は不変。
 * いずれかの失敗時は旧 sequence を保つ。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`UNSUPPORTED`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_sequence_import_mixed_encoded(
    InkpodCore* core,
    const InkpodNamedRasterInput* files,
    uint64_t file_count,
    uint64_t file_stride_bytes);
/**
 * @brief sequence 全 cell を common raster へ encode する。
 * @par 契約
 * Core owner thread。`core`/`out_sequence` は非 NULL、`*out_sequence == NULL`。成功時 Rust-owned immutable sequence handle を格納。
 * 失敗時 NULL のまま。document revision、dirty、Undo、排他状態は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`UNSUPPORTED`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_sequence_export_encoded(
    InkpodCore* core,
    InkpodCommonRasterFormat format,
    uint32_t composite_white,
    InkpodEncodedSequence** out_sequence);
/**
 * @brief encoded sequence の item count を取得する。
 * @par 契約
 * 任意スレッド。`sequence`/`out_count` は非 NULL、release と外部同期。成功時 count をコピー。
 * 所有権、Core、revision、dirty、Undo、排他状態は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_encoded_sequence_count(
    const InkpodEncodedSequence* sequence,
    uint64_t* out_count);
/**
 * @brief encoded sequence item の borrowed name/byte span を取得する。
 * @par 契約
 * 任意スレッド。handle と 4 出力 pointer は非 NULL・非重複、index は範囲内、release と同期。
 * 成功時 span/count を書き、span は sequence release まで有効。失敗時出力未使用。状態変更なし。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_encoded_sequence_get(
    const InkpodEncodedSequence* sequence,
    uint64_t index,
    const uint8_t** out_name,
    uint64_t* out_name_bytes,
    const uint8_t** out_bytes,
    uint64_t* out_byte_count);
/**
 * @brief encoded sequence handle を解放し owner 変数を NULL にする。
 * @par 契約
 * 任意スレッド。owner pointer は非 NULL、`*sequence == NULL` は成功 no-op。成功後すべての borrowed span は無効。
 * Core/revision/dirty/Undo に影響しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_encoded_sequence_release(InkpodEncodedSequence** sequence);
/**
 * @brief sequence cell metadata/name を caller buffer へコピーする。
 * @par 契約
 * Core owner thread。`core`/完全サイズの `output` は非 NULL。name buffer は size-query 対応。
 * success/`BUFFER_TOO_SMALL` の出力規則は `InkpodSequenceCellInfo` を参照。revision、dirty、Undo、排他状態は不変。
 * @par 主なステータス
 * `OK`、`BUFFER_TOO_SMALL`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_sequence_cell_get(
    InkpodCore* core,
    uint32_t index,
    InkpodSequenceCellInfo* output);
/**
 * @brief sequence cell の straight-alpha RGBA8 thumbnail を caller buffer へコピーする。
 * @par 契約
 * Core owner thread。`core`/完全文サイズの `output` は非 NULL。capacity 0/NULL は
 * size query。成功時 metadata/bytes、`BUFFER_TOO_SMALL` 時も required bytes と
 * geometry/checksum を返す。caller-owned storage は保持せず、revision、dirty、Undo、
 * 排他状態は不変。
 * @par 主なステータス
 * `OK`、`BUFFER_TOO_SMALL`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_sequence_thumbnail_get(
    InkpodCore* core,
    uint32_t index,
    InkpodSequenceThumbnailBuffer* output);
/**
 * @brief clean な current document を sequence の index 指定 cell へ切り替える。
 * @par 契約
 * Core owner thread。`core`/完全サイズの `out_info` は非 NULL。dirty なら `UNSAVED_CHANGES` で文書/revision/出力を変えない。
 * 成功時文書を置換し info を書くが Undo item は作らない。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`UNSAVED_CHANGES`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_sequence_activate(
    InkpodCore* core,
    uint32_t index,
    InkpodDocumentInfo* out_info);
/**
 * @brief previous/next の存在する sequence cell へ移動する。
 * @par 契約
 * Core owner thread。`core`/完全サイズの `out_info` は非 NULL、direction/flags は既知値。
 * dirty なら `UNSAVED_CHANGES` で完全不変。成功時文書を置換し info を書くが Undo item は作らない。
 * stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`UNSAVED_CHANGES`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_sequence_step(
    InkpodCore* core,
    InkpodSequenceDirection direction,
    uint32_t flags,
    InkpodDocumentInfo* out_info);
/**
 * @brief 指定 FPS/flags で motion-check session を開始する。
 * @par 契約
 * Core owner thread。`core`/`input`/`out_frame` は非 NULL・非重複、両構造体は完全サイズ、FPS は対応値。
 * 成功時 transient playback state と first frame metadata を作る。document revision、dirty、Undo は不変。
 * 既存 motion/stroke/preview/floating と競合時 `INVALID_STATE`。失敗時 session/output 不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_motion_check_start(
    InkpodCore* core,
    const InkpodMotionCheckInput* input,
    InkpodMotionFrame* out_frame);
/**
 * @brief active motion-check を previous/next frame へ進める。
 * @par 契約
 * Core owner thread。`core`/完全サイズの `out_frame` は非 NULL、direction は既知値。
 * 成功時 playback cursor/output のみ更新し document revision、dirty、Undo は不変。session 不在は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_motion_check_step(
    InkpodCore* core,
    InkpodSequenceDirection direction,
    InkpodMotionFrame* out_frame);
/**
 * @brief motion-check session を停止する。
 * @par 契約
 * Core owner thread。`core` は非 NULL。session 不在は成功 no-op。
 * playback state だけ破棄し document revision、dirty、Undo、stroke/preview は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_motion_check_stop(InkpodCore* core);
/**
 * @brief active motion-check の pause を切り替える。
 * @par 契約
 * Core owner thread。`core`/完全サイズの `out_frame` は非 NULL。成功時 playback flag/output のみ更新。
 * document revision、dirty、Undo は不変。session 不在は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_motion_check_toggle_pause(
    InkpodCore* core,
    InkpodMotionFrame* out_frame);
/**
 * @brief active subpalette index を切り替える。
 * @par 契約
 * Core owner thread。`core` は非 NULL、index は範囲内。成功時 UI palette state のみ更新し document revision、dirty、Undo は不変。
 * 失敗時不変。stroke/preview state を変更しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_subpalette_set(InkpodCore* core, uint32_t index);
/**
 * @brief active subpalette から document 座標 pixel の色を取得する。
 * @par 契約
 * Core owner thread。`core`/完全サイズの `output` は非 NULL。成功時 exact-depth 色だけコピー。
 * query のため revision、dirty、Undo、排他状態は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_subpalette_sample(
    InkpodCore* core,
    uint32_t x,
    uint32_t y,
    InkpodColorValue* output);
/**
 * @brief registered subpalette source 専用 logical view を更新する。
 * @par 契約
 * Core owner thread。`core`/完全サイズの `input` は非 NULL、`view_id` は同じ Core が
 * `inkpod_core_view_create` で発行した live ID。zoom/pan/flip/viewport だけを更新し、
 * editable document、revision、dirty、Undo、active stroke は不変。失敗時 view も不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_subpalette_view_apply(
    InkpodCore* core,
    uint64_t view_id,
    const InkpodViewInput* input);
/**
 * @brief subpalette 専用 view の device-pixel 座標から exact-depth 色を取得する。
 * @par 契約
 * Core owner thread。`core`/完全サイズの `output` は非 NULL、`view_id` は live secondary
 * view。zoom/pan/flip を一度だけ適用し、half-open source bounds 外は `INVALID_ARGUMENT`。
 * query のため document、view、history、dirty、source は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_subpalette_view_sample(
    InkpodCore* core,
    uint64_t view_id,
    double device_x,
    double device_y,
    InkpodColorValue* output);
/**
 * @brief registered subpalette source の read-only immutable snapshot を構築する。
 * @par 契約
 * Core owner thread。`core`/`options`/`out_snapshot` は非 NULL、`view_id` は live secondary
 * view。成功時 Rust-owned snapshot を返し `inkpod_snapshot_release` で解放する。snapshot は
 * Core/sequence の変更や destroy 後も release まで不変。document/history/dirty は不変。
 * 失敗、短い options、不正 ID、subpalette 未登録では owner を NULL のまま返す。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`UNSUPPORTED`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_subpalette_build_snapshot(
    InkpodCore* core,
    uint64_t view_id,
    const InkpodSnapshotOptions* options,
    InkpodSnapshot** out_snapshot);

/**
 * @brief cubic segment span から document-coordinate vector path を追加する。
 * @par 契約
 * Core owner thread。4 pointer は非 NULL・非重複。input/segment span は完全サイズ/stride で呼び出し中だけ borrowed。
 * 成功時 stable path ID/result を書き 1 revision/dirty/Undo 単位。失敗時不変。view zoom は geometry を変えない。
 * stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_vector_add_path(
    InkpodCore* core,
    const InkpodVectorPathInput* input,
    InkpodDispatchResult* result,
    uint64_t* out_path_id);
/**
 * @brief closed boundary path ID span から vector fill を追加する。
 * @par 契約
 * Core owner thread。4 pointer は非 NULL・非重複。input/ID span は一時 borrowed。
 * 成功時 stable fill ID/result を書き 1 revision/dirty/Undo 単位。失敗時 topology/出力不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_vector_add_fill(
    InkpodCore* core,
    const InkpodVectorFillInput* input,
    InkpodDispatchResult* result,
    uint64_t* out_fill_id);
/**
 * @brief partial/to-intersection/whole-path mode で vector を消去する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL・非重複、input は完全サイズ/finite/bounded。
 * 成功時実変更を 1 revision/dirty/Undo 単位、失敗時 topology 不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_vector_erase(
    InkpodCore* core,
    const InkpodVectorEraseInput* input,
    InkpodDispatchResult* result);
/**
 * @brief maximum gap 内の最近傍 endpoint を deterministic に接続する。
 * @par 契約
 * Core owner thread。`core`/`result`/`out_path_id` は非 NULL、plane ID と finite gap を検証。
 * 成功時 ID/result を書き実変更を 1 revision/dirty/Undo 単位。失敗時出力/topology 不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_vector_connect(
    InkpodCore* core,
    uint64_t plane_id,
    float maximum_gap,
    InkpodDispatchResult* result,
    uint64_t* out_path_id);
/**
 * @brief path ID span の線幅を add/subtract/scale/constant で補正する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL、input/ID span は一時 borrowed、parameter は finite/bounded。
 * 成功時実変更を 1 revision/dirty/Undo 単位、失敗時 geometry 不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_vector_correct_width(
    InkpodCore* core,
    const InkpodVectorWidthInput* input,
    InkpodDispatchResult* result);
/**
 * @brief mode/bounds に一致する path ranges と fill IDs を caller buffer へ返す。
 * @par 契約
 * Core owner thread。`core`/`input`/`output` は非 NULL・非重複、構造体は完全サイズ。
 * 各出力 pointer が NULL/capacity 0 なら count query。成功時 complete records、`BUFFER_TOO_SMALL` 時も両必要 count を返す。
 * caller-owned storage は保持しない。query のため revision、dirty、Undo、排他状態は不変。
 * @par 主なステータス
 * `OK`、`BUFFER_TOO_SMALL`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_vector_select(
    InkpodCore* core,
    const InkpodVectorSelectionInput* input,
    InkpodVectorSelectionBuffer* output);
/**
 * @brief vector layer を straight RGBA8 caller buffer へ rasterize する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL・非重複、input/output は完全サイズ。NULL/capacity 0 は size query。
 * 成功時 metadata/bytes、`BUFFER_TOO_SMALL` 時も required bytes/dimensions を返す。document/revision/dirty/Undo は不変。
 * @par 主なステータス
 * `OK`、`BUFFER_TOO_SMALL`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_vector_rasterize(
    InkpodCore* core,
    const InkpodVectorRasterizeInput* input,
    InkpodVectorRasterBuffer* output);
/**
 * @brief vector layer を document scale で新規 RGBA8 raster layer へ変換する。
 * @par 契約
 * Core owner thread。`core`/`input`/非空 UTF-8 name/`result`/`out_layer_id` は非 NULL・非重複。
 * name は一時 borrowed。成功時 source vector を保持して新 stable ID/result を書き、ちょうど 1 revision/dirty/Undo 単位。
 * 失敗時 layer tree/出力不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_vector_rasterize_to_layer(
    InkpodCore* core,
    const InkpodVectorRasterizeInput* input,
    const uint8_t* name_utf8,
    uint64_t name_bytes,
    InkpodDispatchResult* result,
    uint64_t* out_layer_id);
/**
 * @brief raster/color plane の nonzero-alpha row run を vector path/fill topology へ変換する。
 * @par 契約
 * Core owner thread。`core`/`input`/`result`/`out_fill_count` は非 NULL・非重複、input は完全サイズ。
 * `target_layer_id == 0` は `Vectorized` layer 作成と変換を一つの canonical primitive として行う。
 * 成功時 fill count/result を書き 1 revision/dirty/Undo 単位。予測上限超過や失敗時は部分 topology/出力を残さない。
 * stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_raster_vectorize(
    InkpodCore* core,
    const InkpodRasterVectorizeInput* input,
    InkpodDispatchResult* result,
    uint64_t* out_fill_count);

/**
 * @brief filter preview を original committed base から計算して開始する。
 * @par 契約
 * Core owner thread。`core`/`input`/`out_info` は非 NULL・非重複、filter/curve span は一時 borrowed でコピーする。
 * 成功時 1 preview が存在し info/checksum/transient revision を書くが committed revision、dirty、Undo は不変。
 * stroke/既存 preview/floating 中は `INVALID_STATE`。失敗時 preview/output/document 不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_filter_preview_begin(
    InkpodCore* core,
    const InkpodFilterInput* input,
    InkpodFilterPreviewInfo* out_info);
/**
 * @brief cancellable task を使って filter preview を開始する。
 * @par 契約
 * Core owner thread。`core`/`input`/`task`/`out_info` は非 NULL・非重複。task は READY で、Core call 終了まで owner が解放しない。
 * 成功時の preview/revision/dirty/Undo は非 task 版と同じ。cancel は `CANCELLED` で partial preview/output を install しない。
 * stroke/既存 preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`CANCELLED`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_filter_preview_begin_task(
    InkpodCore* core,
    const InkpodFilterInput* input,
    InkpodTask* task,
    InkpodFilterPreviewInfo* out_info);
/**
 * @brief active filter preview を original base から新 parameter で再計算する。
 * @par 契約
 * Core owner thread。`core`/`input`/`out_info` は非 NULL、input span は一時 borrowed。
 * 成功時 transient preview/info のみ置換し committed revision、dirty、Undo は不変。preview 不在、stroke/floating 競合は `INVALID_STATE`。
 * 失敗時従来 preview/output を保つ。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_filter_preview_update(
    InkpodCore* core,
    const InkpodFilterInput* input,
    InkpodFilterPreviewInfo* out_info);
/**
 * @brief cancellable task で active filter preview を original base から再計算する。
 * @par 契約
 * Core owner thread。4 pointer は非 NULL、task は READY で call 終了まで生存。成功時 transient state のみ更新。
 * cancel/失敗時 existing preview を保ち partial result を install しない。committed revision、dirty、Undo は不変。
 * @par 主なステータス
 * `OK`、`CANCELLED`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_filter_preview_update_task(
    InkpodCore* core,
    const InkpodFilterInput* input,
    InkpodTask* task,
    InkpodFilterPreviewInfo* out_info);
/**
 * @brief active filter/dust preview を破棄する。
 * @par 契約
 * Core owner thread。`core`/完全サイズの `out_info` は非 NULL。成功時 base checksum/info を書き preview を終了。
 * committed revision、dirty、Undo は begin 前から不変。preview 不在は `INVALID_STATE`。失敗時 state/output 不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_filter_preview_cancel(
    InkpodCore* core,
    InkpodFilterPreviewInfo* out_info);
/**
 * @brief active filter/dust preview を committed document へ適用する。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL。成功時 preview を終了し、実変更をちょうど 1 revision/dirty/Undo 単位で commit、
 * last-filter も記録する。preview 不在は `INVALID_STATE`。失敗時 committed base と preview を保つ。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_filter_preview_apply(
    InkpodCore* core,
    InkpodDispatchResult* result);
/**
 * @brief 最後に apply した filter を指定 plane へ再適用する。
 * @par 契約
 * Core owner thread。`core`/`result` は非 NULL、plane ID と copied last-filter が必要。
 * 成功時実変更を 1 revision/dirty/Undo 単位、失敗時不変。stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_filter_apply_last(
    InkpodCore* core,
    uint64_t plane_id,
    InkpodDispatchResult* result);
/**
 * @brief cancellable task で last filter を再適用する。
 * @par 契約
 * Core owner thread。`core`/`task`/`result` は非 NULL、task は READY で call 終了まで生存。
 * 成功時 1 revision/dirty/Undo 単位。cancel/失敗時 partial edit/result を commit しない。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`CANCELLED`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_filter_apply_last_task(
    InkpodCore* core,
    uint64_t plane_id,
    InkpodTask* task,
    InkpodDispatchResult* result);
/**
 * @brief supported filter parameter から非破壊 adjustment layer を作成する。
 * @par 契約
 * Core owner thread。`core`/`input`/非空 UTF-8 name/`result`/`out_layer_id` は非 NULL・非重複。
 * input curve/name は一時 borrowed でコピー。成功時 stable ID/result を書き 1 revision/dirty/Undo 単位。
 * source raster は変更しない。失敗時不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`UNSUPPORTED`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_adjustment_create(
    InkpodCore* core,
    const InkpodFilterInput* input,
    const uint8_t* name_utf8,
    uint64_t name_length,
    InkpodDispatchResult* result,
    uint64_t* out_layer_id);
/**
 * @brief 既存 adjustment layer の copied filter parameter を置換する。
 * @par 契約
 * Core owner thread。`core`/`input`/`result` は非 NULL、layer ID は adjustment layer。input span は一時 borrowed。
 * 成功時 1 revision/dirty/Undo 単位。失敗時 parameter/document 不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`UNSUPPORTED`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_adjustment_update(
    InkpodCore* core,
    uint64_t layer_id,
    const InkpodFilterInput* input,
    InkpodDispatchResult* result);
/**
 * @brief linear/radial gradient を target plane へ適用する。
 * @par 契約
 * Core owner thread。`core`/`input`/`result` は非 NULL・非重複、stop span は一時 borrowed、geometry/色/mode は検証する。
 * 成功時 1 revision/dirty/Undo 単位。失敗時 partial gradient なし。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_effect_gradient(
    InkpodCore* core,
    const InkpodGradientInput* input,
    InkpodDispatchResult* result);
/**
 * @brief 1 primitive airbrush dab を target plane へ適用する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL、input は完全サイズ/finite/bounded borrowed。
 * 成功時実変更を 1 revision/dirty/Undo 単位。失敗時不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_effect_airbrush(
    InkpodCore* core,
    const InkpodAirbrushInput* input,
    InkpodDispatchResult* result);
/**
 * @brief 指定色境界へ boundary airbrush を適用する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL、input/color span は完全サイズ/stride で一時 borrowed。
 * 成功時 1 revision/dirty/Undo 単位。失敗時 atomic に不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_effect_boundary_airbrush(
    InkpodCore* core,
    const InkpodBoundaryAirbrushInput* input,
    InkpodDispatchResult* result);
/**
 * @brief blur primitive を target plane/selection へ適用する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL、input は完全サイズ/bounded。
 * 成功時実変更を 1 revision/dirty/Undo 単位。失敗時 partial blur なし。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_effect_blur(
    InkpodCore* core,
    const InkpodBlurEffectInput* input,
    InkpodDispatchResult* result);
/**
 * @brief source rectangle を destination へ stamp する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL、input は完全サイズ/bounded。
 * 成功時実変更を 1 revision/dirty/Undo 単位。失敗時 overlap を含め atomic に不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_effect_stamp(
    InkpodCore* core,
    const InkpodStampInput* input,
    InkpodDispatchResult* result);
/**
 * @brief sample span 全体を 1 airbrush gesture として適用する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL、style/sample span は完全サイズ/stride で一時 borrowed。
 * device 座標は view で 1 回変換。成功時 gesture 全体が高々 1 revision/dirty/Undo 単位。失敗時不変。
 * stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_effect_airbrush_gesture(
    InkpodCore* core,
    const InkpodAirbrushGestureInput* input,
    InkpodDispatchResult* result);
/**
 * @brief source point と destination sample span を 1 stamp gesture として適用する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL、input/sample span は一時 borrowed。device 座標は指定 view で変換。
 * 成功時 gesture 全体が高々 1 revision/dirty/Undo 単位。失敗時不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_effect_stamp_gesture(
    InkpodCore* core,
    const InkpodStampGestureInput* input,
    InkpodDispatchResult* result);
/**
 * @brief pen/rectangle/polyline/lasso region へ blur tool gesture を適用する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL、input/sample span は完全サイズ/stride で一時 borrowed。
 * 成功時 tool gesture 全体が高々 1 revision/dirty/Undo 単位。失敗時不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_effect_blur_tool(
    InkpodCore* core,
    const InkpodBlurToolInput* input,
    InkpodDispatchResult* result);
/**
 * @brief cancellable task で dust removal を直接適用する。
 * @par 契約
 * Core owner thread。`core`/`input`/`task`/`result` は非 NULL、input/sample span は一時 borrowed、task は READY で call 終了まで生存。
 * 成功時全処理を 1 revision/dirty/Undo 単位。cancel/失敗時 partial edit/result を commit しない。
 * stroke/preview/floating 中は `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`CANCELLED`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_dust_remove(
    InkpodCore* core,
    const InkpodDustInput* input,
    InkpodTask* task,
    InkpodDispatchResult* result);
/**
 * @brief cancellable task で dust removal preview を開始する。
 * @par 契約
 * Core owner thread。4 pointer は非 NULL、input span は一時 borrowed、task は READY で call 終了まで生存。
 * 成功時 transient preview/info のみで committed revision、dirty、Undo は不変。cancel/失敗時 preview を install しない。
 * stroke/既存 preview/floating 中は `INVALID_STATE`。apply/cancel は filter preview API を使う。
 * @par 主なステータス
 * `OK`、`CANCELLED`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_dust_preview_begin(
    InkpodCore* core,
    const InkpodDustInput* input,
    InkpodTask* task,
    InkpodFilterPreviewInfo* out_info);
/**
 * @brief grayscale8/16 raster で target plane の alpha だけを置換する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL、input pixels は完全サイズ/row-range で呼び出し中だけ borrowed。
 * 成功時 RGB を保ち実変更を 1 revision/dirty/Undo 単位。失敗時 partial alpha edit なし。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_alpha_edit(
    InkpodCore* core,
    const InkpodAlphaEditInput* input,
    InkpodDispatchResult* result);
/**
 * @brief gradient 値を target plane の alpha channel だけへ適用する。
 * @par 契約
 * Core owner thread。3 pointer は非 NULL、input/stop span は完全サイズ/stride で一時 borrowed。
 * 成功時 RGB を保ち 1 revision/dirty/Undo 単位。失敗時不変。stroke/preview/floating 中は不可。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_alpha_gradient(
    InkpodCore* core,
    const InkpodGradientInput* input,
    InkpodDispatchResult* result);

/**
 * @brief READY 状態の thread-safe task を作成する。
 * @par 契約
 * 任意スレッド。`out_task` は非 NULL、`*out_task == NULL`。成功時 Rust-owned handle、失敗時 NULL のまま。
 * Core/document revision、dirty、Undo、stroke/preview に影響しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_task_create(InkpodTask** out_task);
/**
 * @brief task の atomic state/progress を取得する。
 * @par 契約
 * 任意スレッド。`task`/完全サイズの `out_info` は非 NULL、task release と外部同期。
 * Core operation と同時 query 可。成功時 snapshot 値をコピー、失敗時出力未使用。文書状態は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_task_query(
    const InkpodTask* task,
    InkpodTaskInfo* out_info);
/**
 * @brief task へ thread-safe な cancellation を要求する。
 * @par 契約
 * 任意スレッド。`task` は非 NULL borrowed、release と外部同期。Core operation と同時可。
 * 成功は要求記録を意味し、Core が poll 後 `CANCELLED` で staged result を破棄する。revision、dirty、Undo を直接変えない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_task_cancel(InkpodTask* task);
/**
 * @brief task handle を解放し owner 変数を NULL にする。
 * @par 契約
 * 任意スレッド。owner pointer は非 NULL、`*task == NULL` は成功 no-op。Core call が戻る前に release してはならない。
 * 成功後別名は無効。document revision、dirty、Undo、preview を変えない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_task_release(InkpodTask** task);

/**
 * @brief nested input/operation span を検証・コピーして immutable batch graph を作る。
 * @par 契約
 * 任意スレッド。`input`/`out_graph` は非 NULL・非重複、全 nested structure/stride/string/span は完全で一時 borrowed、
 * `*out_graph == NULL`。成功時 Rust-owned graph、失敗時 NULL のまま。Core/revision/dirty/Undo/排他状態に影響しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`UNSUPPORTED`、`PANIC`。
 */
/** Copies the exact UUID/generation identity of one natural-order sequence source. */
InkpodStatus inkpod_core_sequence_source_identity(
    InkpodCore* core,
    uint32_t sequence_index,
    InkpodSequenceSourceIdentity* out_identity);

/**
 * Compares two immutable sequence rasters at identical document coordinates.
 * Both identities are borrowed for the call. On success `*out_preview` owns a
 * Rust allocation and must be released with `inkpod_batch_pair_preview_release`.
 * Stale/missing identities, equal identities, dimensions/formats that differ,
 * non-NULL output ownership, and wrong-thread access are rejected atomically.
 */
InkpodStatus inkpod_core_batch_extract_color_pairs(
    InkpodCore* core,
    const InkpodSequenceSourceIdentity* old_identity,
    const InkpodSequenceSourceIdentity* new_identity,
    InkpodBatchPairPreview** out_preview);

/** Copies pair-preview counts and native raster metadata into caller storage. */
InkpodStatus inkpod_batch_pair_preview_get_info(
    const InkpodBatchPairPreview* preview,
    InkpodBatchPairPreviewInfo* out_info);

/** Copies one exact candidate and half-open document bounds into caller storage. */
InkpodStatus inkpod_batch_pair_preview_get_candidate(
    const InkpodBatchPairPreview* preview,
    uint64_t index,
    InkpodBatchPairCandidate* out_candidate);

/** Releases an owned preview and sets the caller owner pointer to NULL; NULL is a no-op. */
InkpodStatus inkpod_batch_pair_preview_release(InkpodBatchPairPreview** preview);

InkpodStatus inkpod_batch_graph_create(
    const InkpodBatchGraphInput* input,
    InkpodBatchGraph** out_graph);
/**
 * @brief versioned/checksummed `.inkbatch` を immutable graph として読む。
 * @par 契約
 * 任意スレッド。非空 UTF-8 path と `out_graph` は非 NULL・非重複、path は呼び出し中だけ borrowed、`*out_graph == NULL`。
 * 成功時 Rust-owned graph、失敗時 NULL のまま。Core/document state は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`IO_ERROR`、`UNSUPPORTED`、`PANIC`。
 */
InkpodStatus inkpod_batch_graph_load(
    const uint8_t* path_utf8,
    uint64_t path_bytes,
    InkpodBatchGraph** out_graph);
/**
 * @brief immutable graph を `.inkbatch` へ atomic 保存する。
 * @par 契約
 * 任意スレッド。`graph`/非空 UTF-8 path は非 NULL・非重複、path は一時 borrowed。
 * 同 directory の一時 file 完成後に置換。成功/失敗とも graph 所有権と Core/revision/dirty/Undo は不変、失敗時既存 file を保つ。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`IO_ERROR`、`PANIC`。
 */
InkpodStatus inkpod_batch_graph_save(
    const InkpodBatchGraph* graph,
    const uint8_t* path_utf8,
    uint64_t path_bytes);
/**
 * @brief immutable graph の version/count/policy をコピーする。
 * @par 契約
 * 任意スレッド。`graph`/完全サイズの `out_info` は非 NULL、release と外部同期。
 * 成功時だけ caller-owned 出力を初期化。所有権と文書状態は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_batch_graph_get_info(
    const InkpodBatchGraph* graph,
    InkpodBatchGraphInfo* out_info);
/** Copies one operation's scalar fields and nested row counts. */
InkpodStatus inkpod_batch_graph_get_operation(
    const InkpodBatchGraph* graph,
    uint64_t index,
    InkpodBatchOperationInfo* out_info);
/** Copies one separation/boundary-airbrush color row. */
InkpodStatus inkpod_batch_graph_get_operation_color(
    const InkpodBatchGraph* graph,
    uint64_t operation_index,
    uint64_t color_index,
    InkpodColorValue* out_color);
/** Copies one color-replacement row. */
InkpodStatus inkpod_batch_graph_get_operation_color_pair(
    const InkpodBatchGraph* graph,
    uint64_t operation_index,
    uint64_t pair_index,
    InkpodBatchColorPairInput* out_pair);
/** Copies one continuous-fill seed row. */
InkpodStatus inkpod_batch_graph_get_operation_seed(
    const InkpodBatchGraph* graph,
    uint64_t operation_index,
    uint64_t seed_index,
    InkpodBatchSeedInput* out_seed);
/** Copies one tone-curve point. */
InkpodStatus inkpod_batch_graph_get_operation_curve_point(
    const InkpodBatchGraph* graph,
    uint64_t operation_index,
    uint64_t point_index,
    InkpodCurvePoint* out_point);
/**
 * Creates an immutable run graph by replacing all source operations with a
 * complete borrowed operation span. Count must match and every per-run flag
 * must already be cleared. Success transfers a new Rust-owned graph handle.
 */
InkpodStatus inkpod_batch_graph_clone_with_operations(
    const InkpodBatchGraph* graph,
    const InkpodBatchOperationInput* operations,
    uint64_t operation_count,
    uint64_t operation_stride_bytes,
    InkpodBatchGraph** out_graph);
/**
 * @brief batch graph handle を解放し owner 変数を NULL にする。
 * @par 契約
 * 任意スレッド。owner pointer は非 NULL、`*graph == NULL` は成功 no-op。preview/execute 使用中に解放しない。
 * 成功後別名は無効。Core/revision/dirty/Undo に影響しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_batch_graph_release(InkpodBatchGraph** graph);
/**
 * @brief graph/scope から自然順 input/output/warning の immutable preview を作る。
 * @par 契約
 * Core owner thread。`core`/`graph`/`out_preview` は非 NULL・非重複、`*out_preview == NULL`。graph は call 中 borrowed。
 * 成功時 Rust-owned preview。失敗時 NULL のまま。dry preview のため current document revision、dirty、Undo は不変。
 * stroke/filter preview/floating と競合時 `INVALID_STATE`。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INVALID_STATE`、`WRONG_THREAD`、`IO_ERROR`、`PANIC`。
 */
InkpodStatus inkpod_core_batch_preview(
    InkpodCore* core,
    const InkpodBatchGraph* graph,
    InkpodBatchRunScope scope,
    InkpodBatchPreview** out_preview);
/**
 * @brief batch preview の item count を取得する。
 * @par 契約
 * 任意スレッド。`preview`/`out_count` は非 NULL、release と外部同期。成功時 count をコピー。
 * ownership/Core/revision/dirty/Undo は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_batch_preview_count(
    const InkpodBatchPreview* preview,
    uint64_t* out_count);
/**
 * @brief batch preview 1 item の borrowed UTF-8 spans を取得する。
 * @par 契約
 * 任意スレッド。`preview`/完全サイズの `out_item` は非 NULL、index は範囲内、release と同期。
 * 成功時 spans を書き、親 preview release まで有効。失敗時出力未使用。文書状態不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_batch_preview_get(
    const InkpodBatchPreview* preview,
    uint64_t index,
    InkpodBatchPreviewItem* out_item);
/**
 * @brief batch preview handle を解放し owner 変数を NULL にする。
 * @par 契約
 * 任意スレッド。owner pointer は非 NULL、`*preview == NULL` は成功 no-op。成功後 item/span は無効。
 * Core/revision/dirty/Undo に影響しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_batch_preview_release(InkpodBatchPreview** preview);
/**
 * @brief graph を current/all scope で dry-run または実行し immutable report を返す。
 * @par 契約
 * Core owner thread。`core`/`graph`/`task`/`out_report` は非 NULL・非重複、`*out_report == NULL`。
 * task は READY で call 終了まで生存。成功時 per-output atomic save と Rust-owned report。
 * cancel 時は `CANCELLED` でも利用可能な owned report を返し得るため、caller は status にかかわらず `*out_report` を確認して解放する。
 * current Core document revision、dirty、Undo は変えない。stroke/preview/floating 中は `INVALID_STATE`。失敗 item は既存 output を壊さない。
 * @par 主なステータス
 * `OK`、`CANCELLED`、`INVALID_ARGUMENT`、`INVALID_STATE`、`IO_ERROR`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_batch_execute(
    InkpodCore* core,
    const InkpodBatchGraph* graph,
    InkpodBatchRunScope scope,
    uint64_t flags,
    InkpodBatchTask* task,
    InkpodBatchReport** out_report);
/**
 * @brief batch report の cancelled/item/failure count をコピーする。
 * @par 契約
 * 任意スレッド。`report`/完全サイズの `out_info` は非 NULL、release と外部同期。
 * 成功時だけ出力を初期化。ownership/Core/revision/dirty/Undo は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_batch_report_get_info(
    const InkpodBatchReport* report,
    InkpodBatchReportInfo* out_info);
/**
 * @brief batch report 1 item の outcome と borrowed UTF-8 spans を取得する。
 * @par 契約
 * 任意スレッド。`report`/完全サイズの `out_item` は非 NULL、index は範囲内、release と同期。
 * 成功時 spans を書き、親 report release まで有効。失敗時出力未使用。文書状態不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_batch_report_get(
    const InkpodBatchReport* report,
    uint64_t index,
    InkpodBatchReportItem* out_item);
/**
 * @brief batch report handle を解放し owner 変数を NULL にする。
 * @par 契約
 * 任意スレッド。owner pointer は非 NULL、`*report == NULL` は成功 no-op。成功後 item/span は無効。
 * Core/revision/dirty/Undo に影響しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_batch_report_release(InkpodBatchReport** report);
/**
 * @brief READY 状態の thread-safe batch task を作る。
 * @par 契約
 * 任意スレッド。`out_task` は非 NULL、`*out_task == NULL`。成功時 Rust-owned task、失敗時 NULL。
 * Core/revision/dirty/Undo/排他状態に影響しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_batch_task_create(InkpodBatchTask** out_task);
/**
 * @brief batch task の atomic state/progress を取得する。
 * @par 契約
 * 任意スレッド。`task`/完全サイズの `out_info` は非 NULL、execute と同時可、release と同期。
 * 成功時 snapshot 値をコピー。文書状態不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_batch_task_query(
    const InkpodBatchTask* task,
    InkpodTaskInfo* out_info);
/**
 * @brief batch task へ thread-safe cancellation を要求する。
 * @par 契約
 * 任意スレッド。`task` は非 NULL borrowed、execute と同時可、release と同期。
 * Core が poll すると staged item を commit せず `CANCELLED` report へ反映する。current document 状態を直接変えない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_batch_task_cancel(InkpodBatchTask* task);
/**
 * @brief batch task handle を解放し owner 変数を NULL にする。
 * @par 契約
 * 任意スレッド。owner pointer は非 NULL、`*task == NULL` は成功 no-op。execute が戻る前に release しない。
 * 成功後別名は無効。文書状態不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_batch_task_release(InkpodBatchTask** task);

/**
 * @brief 指定 logical view の immutable render snapshot を構築する。
 * @par 契約
 * Core owner thread。`core`/`options`/`out_snapshot` は非 NULL・非重複、options は完全サイズ borrowed、
 * `*out_snapshot == NULL`。成功時 Rust-owned snapshot。live stroke/filter/dust/floating preview 中も許可し transient content を capture する。
 * snapshot build 自体は document revision、dirty、Undo を変えない。失敗時 owner は NULL のまま。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_build_snapshot_for_view(
    InkpodCore* core,
    uint64_t view_id,
    const InkpodSnapshotOptions* options,
    InkpodSnapshot** out_snapshot);

/**
 * @brief primary view の immutable render snapshot を構築する。
 * @par 契約
 * Core owner thread。`core`/`options`/`out_snapshot` は非 NULL・非重複、options は完全サイズ borrowed、
 * `*out_snapshot == NULL`。成功時 Rust-owned snapshot を格納し、Core destroy 後も独立して生存できる。
 * live stroke/filter/dust/floating preview を capture できるが、build は committed revision、dirty、Undo を変えない。失敗時 NULL のまま。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`NO_DOCUMENT`、`WRONG_THREAD`、`PANIC`。
 */
InkpodStatus inkpod_core_build_snapshot(
    InkpodCore* core,
    const InkpodSnapshotOptions* options,
    InkpodSnapshot** out_snapshot);

/**
 * @brief snapshot の raster tile view を取得する。
 * @par 契約
 * 任意スレッド。`snapshot`/完全サイズの `out_view` は非 NULL・非重複、同じ snapshot の release と同期。
 * 成功時 tile span/pixels は親 snapshot 所有の borrowed pointer として返り release まで有効。失敗時出力未使用。
 * accessor は Core/revision/dirty/Undo/stroke/preview を変えない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_snapshot_get_view(
    const InkpodSnapshot* snapshot,
    InkpodSnapshotView* out_view);

/**
 * @brief snapshot が固定した view transform を値コピーする。
 * @par 契約
 * 任意スレッド。`snapshot`/完全サイズの `out_transform` は非 NULL、release と同期。
 * 成功時だけ出力を初期化。borrowed nested pointer はなく、状態変更なし。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_snapshot_get_transform(
    const InkpodSnapshot* snapshot,
    InkpodSnapshotTransform* out_transform);

/**
 * @brief snapshot の overlay/grid と borrowed guide span を取得する。
 * @par 契約
 * 任意スレッド。`snapshot`/完全サイズの `out_overlay` は非 NULL、release と同期。
 * 成功時 guide span は snapshot release まで有効。失敗時出力未使用。Core/revision/dirty/Undo は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_snapshot_get_overlay(
    const InkpodSnapshot* snapshot,
    InkpodSnapshotOverlay* out_overlay);

/**
 * @brief snapshot の vector segment/fill/boundary-ID spans を取得する。
 * @par 契約
 * 任意スレッド。`snapshot`/完全サイズの `out_vectors` は非 NULL、release と同期。
 * 成功時全 span は snapshot-owned borrowed で release まで有効。fill の範囲は boundary_path_ids を index し、segment は path ID ごとに並ぶ。
 * 失敗時出力未使用。状態変更なし。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_snapshot_get_vectors(
    const InkpodSnapshot* snapshot,
    InkpodSnapshotVectorView* out_vectors);

/**
 * @brief snapshot の view-local vector 診断設定と未接続端点 span を取得する。
 * @par 契約
 * 任意スレッド。`snapshot`/完全サイズの `out_diagnostics` は非 NULL、release と同期。
 * 成功時 endpoint span は snapshot-owned borrowed で release まで有効。文書、履歴、dirty は変更しない。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`INCOMPATIBLE_ABI`、`PANIC`。
 */
InkpodStatus inkpod_snapshot_get_vector_diagnostics(
    const InkpodSnapshot* snapshot,
    InkpodSnapshotVectorDiagnostics* out_diagnostics);

/**
 * @brief Copies the bottom-to-top render plan for a live immutable snapshot.
 * @par Contract
 * `snapshot` and exact-size `out_plan` are non-NULL and externally synchronized.
 * Returned spans are snapshot-owned and valid until release. Pass item ranges
 * index the tile, vector-fill, vector-segment, or adjustment-LUT span selected
 * by `kind`. Unknown kinds are not emitted. Failure does not partially write
 * document state or transfer ownership.
 */
InkpodStatus inkpod_snapshot_get_render_plan(
    const InkpodSnapshot* snapshot,
    InkpodSnapshotRenderPlan* out_plan);

/**
 * @brief Copies the fixed build/replay contract on the Core owner thread.
 * @par Contract
 * `core` remains caller-owned. `out_contract` is caller-owned and must have
 * exact current `struct_size`, zero `reserved`, and no unknown feature flags.
 * Success changes no Core state; failure does not partially write the record.
 */
InkpodStatus inkpod_core_get_replay_contract(
    InkpodCore* core,
    InkpodReplayContract* out_contract);

/**
 * @brief Copies the view-independent canonical document-result digest.
 * @par Contract
 * The immutable snapshot remains Rust-owned and must stay live for the whole
 * call. This query may run on any snapshot read thread, transfers no ownership,
 * and changes no Core or snapshot state. Concurrent release is invalid.
 * `out_digest` is caller-owned with exact current `struct_size`; failure does
 * not partially write the record.
 */
InkpodStatus inkpod_snapshot_get_canonical_digest(
    const InkpodSnapshot* snapshot,
    InkpodCanonicalDigest* out_digest);

/**
 * @brief snapshot の Rust 所有権を解放し owner 変数を NULL にする。
 * @par 契約
 * 任意の外部同期済み renderer thread。owner pointer は非 NULL、`*snapshot == NULL` は成功 no-op。
 * 成功後 tile/pixel/guide/vector を含む全 borrowed alias は無効。Core/document revision、dirty、Undo、preview は不変。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_snapshot_release(InkpodSnapshot** snapshot);

/**
 * @brief Core handleのgeneration付きABI-v3 identityを値コピーする。
 * @par 契約 Core owner thread。`out_id`は完全サイズの空ID。queryで状態変更なし。
 */
InkpodStatus inkpod_core_get_id_v3(InkpodCore* core, InkpodObjectId* out_id);

/**
 * @brief exact-depth color spanをRust-owned immutable objectへ同期コピーする。
 * @par 契約 Core owner thread。戻り後はinput/spanを変更・解放可能。成功IDは同じCore generationで
 * primitive payloadとして使い、`inkpod_core_object_release_v3`またはCore destroyまで有効。
 */
InkpodStatus inkpod_core_register_color_array_v3(
    InkpodCore* core,
    const InkpodColorArray* input,
    InkpodObjectId* out_id);

/** @brief strided stroke sample spanをRust-owned immutable sample-stream IDへ同期コピーする。 */
InkpodStatus inkpod_core_register_sample_stream_v3(
    InkpodCore* core,
    const InkpodStrokeSampleSpan* input,
    InkpodObjectId* out_id);

/** @brief bounded raster spanをpadding除去済みRust-owned immutable asset IDへ同期コピーする。 */
InkpodStatus inkpod_core_register_raster_asset_v3(
    InkpodCore* core,
    const InkpodRasterAssetInputV3* input,
    InkpodObjectId* out_id);

/**
 * @brief stable opcode/version/value/ID recordを唯一のCore canonical executorで実行する。
 * @par 契約 Core owner thread。requestはpointer-freeで、payloadは同generationの正しいtypeのlive ID。
 * success実変更は1 revision/1 history/1 procedure、semantic no-opはCOMMITTEDなしでrevision不変。
 * invalid/stale/wrong-type/wrong-generation/active-preview failureはCoreと全IDを変更しない。
 */
InkpodStatus inkpod_core_primitive_execute_v3(
    InkpodCore* core,
    const InkpodPrimitiveRequestV3* request,
    InkpodPrimitiveResultV3* result);

/** @brief primary render snapshotをCore generation所有のimmutable snapshot IDとして構築する。 */
InkpodStatus inkpod_core_build_snapshot_id_v3(
    InkpodCore* core,
    const InkpodSnapshotOptions* options,
    InkpodObjectId* out_id);

/** @brief live snapshot IDのpointer-free metadataを値コピーする。 */
InkpodStatus inkpod_core_snapshot_get_info_v3(
    InkpodCore* core,
    const InkpodObjectId* id,
    InkpodSnapshotInfoV3* out_info);

/** @brief snapshot tile descriptorsを`first`からcaller-owned strided batchへbounded copyする。 */
InkpodStatus inkpod_core_snapshot_tiles_copy_v3(
    InkpodCore* core,
    const InkpodObjectId* id,
    uint64_t first,
    InkpodSnapshotTileInfoV3* output,
    uint64_t capacity,
    uint64_t stride_bytes,
    uint64_t* out_copied);

/** @brief 一つのsnapshot tile pixel列をoffset付きcaller bufferへbounded copyする。 */
InkpodStatus inkpod_core_snapshot_tile_pixels_copy_v3(
    InkpodCore* core,
    const InkpodObjectId* id,
    uint64_t tile_index,
    InkpodBufferCopyV3* copy);

/** @brief snapshot guide recordsをcaller-owned strided batchへbounded copyする。 */
InkpodStatus inkpod_core_snapshot_guides_copy_v3(
    InkpodCore* core,
    const InkpodObjectId* id,
    uint64_t first,
    InkpodSnapshotGuide* output,
    uint64_t capacity,
    uint64_t stride_bytes,
    uint64_t* out_copied);

/** @brief snapshot vector segment recordsをcaller-owned strided batchへbounded copyする。 */
InkpodStatus inkpod_core_snapshot_vector_segments_copy_v3(
    InkpodCore* core,
    const InkpodObjectId* id,
    uint64_t first,
    InkpodSnapshotVectorSegment* output,
    uint64_t capacity,
    uint64_t stride_bytes,
    uint64_t* out_copied);

/** @brief snapshot vector fill recordsをcaller-owned strided batchへbounded copyする。 */
InkpodStatus inkpod_core_snapshot_vector_fills_copy_v3(
    InkpodCore* core,
    const InkpodObjectId* id,
    uint64_t first,
    InkpodSnapshotVectorFill* output,
    uint64_t capacity,
    uint64_t stride_bytes,
    uint64_t* out_copied);

/** @brief snapshot vector boundary path IDsをcaller-owned strided batchへbounded copyする。 */
InkpodStatus inkpod_core_snapshot_vector_boundary_ids_copy_v3(
    InkpodCore* core,
    const InkpodObjectId* id,
    uint64_t first,
    uint64_t* output,
    uint64_t capacity,
    uint64_t stride_bytes,
    uint64_t* out_copied);

/** @brief layer thumbnail RGBA8をRust-owned thumbnail IDとして生成する。 */
InkpodStatus inkpod_core_layer_thumbnail_id_v3(
    InkpodCore* core,
    uint64_t layer_id,
    uint32_t maximum_width,
    uint32_t maximum_height,
    InkpodObjectId* out_id);

/** @brief common raster export bytesをRust-owned export IDとして生成する。 */
InkpodStatus inkpod_core_export_common_raster_id_v3(
    InkpodCore* core,
    InkpodCommonRasterFormat format,
    uint32_t composite_white,
    InkpodObjectId* out_id);

/** @brief live ABI-v3 objectのtype/generation/count/byte metadataを値コピーする。 */
InkpodStatus inkpod_core_object_get_info_v3(
    InkpodCore* core,
    const InkpodObjectId* id,
    InkpodObjectInfoV3* out_info);

/** @brief thumbnail/export IDのbytesをoffset付きcaller bufferへbounded copyする。 */
InkpodStatus inkpod_core_object_bytes_copy_v3(
    InkpodCore* core,
    const InkpodObjectId* id,
    InkpodBufferCopyV3* copy);

/** @brief 同generationに属するRust-owned task IDを生成する。 */
InkpodStatus inkpod_core_task_create_v3(InkpodCore* core, InkpodObjectId* out_id);
/** @brief task IDのatomic state/progressを値コピーする。 */
InkpodStatus inkpod_core_task_query_v3(
    InkpodCore* core,
    const InkpodObjectId* id,
    InkpodTaskInfo* out_info);
/** @brief task IDをcooperative cancelled stateへ遷移させる。 */
InkpodStatus inkpod_core_task_cancel_v3(InkpodCore* core, const InkpodObjectId* id);

/**
 * @brief live ABI-v3 object IDをexactly onceで解放する。
 * @par 契約 Core owner thread。同じ値の再releaseは`INVALID_STATE`。Core IDは個別release不可。
 * 成功後ID値はstaleとなり、borrow/copy期限は直ちに終了する。Core destroyは残存objectを全解放する。
 */
InkpodStatus inkpod_core_object_release_v3(
    InkpodCore* core,
    const InkpodObjectId* id);

/**
 * @brief current thread の直近 FFI diagnostic に必要な UTF-8 buffer size を得る。
 * @par 契約
 * 任意スレッド。`out_required_bytes` は非 NULL。成功時 trailing NUL を含む byte 数をコピー。
 * Core/stroke/preview/revision/dirty/Undo と所有権は不変。失敗時出力未使用。
 * @par 主なステータス
 * `OK`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_error_message_size(uint64_t* out_required_bytes);
/**
 * @brief current thread の直近 FFI diagnostic を caller-owned UTF-8 buffer へコピーする。
 * @par 契約
 * 任意スレッド。`buffer` と `out_written_bytes` は非 NULL・非重複。capacity は NUL を含む必要量以上。
 * 成功時 NUL terminate し、`out_written_bytes` は NUL を除く byte 数。失敗時 written=0、diagnostic は再取得可能なまま。
 * Core/stroke/preview/revision/dirty/Undo は不変。
 * @par 主なステータス
 * `OK`、`BUFFER_TOO_SMALL`、`INVALID_ARGUMENT`、`PANIC`。
 */
InkpodStatus inkpod_error_message_copy(
    uint8_t* buffer,
    uint64_t buffer_capacity,
    uint64_t* out_written_bytes);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* INKPOD_CORE_FFI_H */
