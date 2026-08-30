use super::*;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

const STATE_MACHINE_UUID: u128 = 0x494e_4b50_4f44_2d4d_312d_5354_4154_4501;
const STATE_MACHINE_SEEDS: [u64; 3] = [
    0x494e_4b50_4f44_0001,
    0x494e_4b50_4f44_0002,
    0x494e_4b50_4f44_0003,
];

#[derive(Clone, Debug, PartialEq)]
struct SemanticTileObservation {
    tile_id: u64,
    origin_x: i32,
    origin_y: i32,
    width: u32,
    height: u32,
    stride_bytes: u32,
    pixel_checksum: u64,
}

#[derive(Clone, Debug, PartialEq)]
struct TileObservation {
    semantic: SemanticTileObservation,
    tile_revision: u64,
}

#[derive(Clone, Debug, PartialEq)]
struct SemanticSnapshotObservation {
    revision: u64,
    feature_flags: u64,
    view: ViewState,
    document_width: u32,
    document_height: u32,
    guides: Vec<Guide>,
    grid: GridConfig,
    tiles: Vec<TileObservation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HistoryObservation {
    entries: Vec<HistoryEntryInfo>,
    cursor: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct CommonObservation {
    document: DocumentInfo,
    topology: Vec<LayerInfo>,
    history: HistoryObservation,
    main_view: ViewState,
    snapshot: SemanticSnapshotObservation,
}

#[derive(Clone, Debug, PartialEq)]
struct FeatureObservation {
    selection_bounds: Option<RectI32>,
    palette: Vec<PixelValue>,
    main_line_color: PixelValue,
    guides: Vec<Guide>,
    grid: GridConfig,
    light_table_sets: Vec<LightTableSetInfo>,
    light_table_items: Vec<LightTableItemInfo>,
}

#[derive(Clone, Debug, PartialEq)]
struct CoreObservation {
    common: CommonObservation,
    features: FeatureObservation,
}

#[derive(Clone, Debug, PartialEq)]
struct DocumentSemanticObservation {
    document_id: u64,
    document_uuid: u128,
    width: u32,
    height: u32,
    dpi_x_milli: u32,
    dpi_y_milli: u32,
    frames: FrameMetadata,
    main_plane_checksum: u64,
    color_plane_checksum: u64,
    topology: Vec<LayerInfo>,
    snapshot_tiles: Vec<SemanticTileObservation>,
    features: FeatureObservation,
}

impl CoreObservation {
    fn capture(core: &mut Core) -> Result<Self, CoreError> {
        let document = core.document_info()?;
        let topology = core.layers()?;
        let history = HistoryObservation {
            entries: core.history_entries(),
            cursor: core.history_cursor(),
        };
        let main_view = core.view_state();
        let snapshot = core.build_snapshot();
        let snapshot = SemanticSnapshotObservation {
            revision: snapshot.revision(),
            feature_flags: snapshot.feature_flags(),
            view: snapshot.view(),
            document_width: snapshot.document_width(),
            document_height: snapshot.document_height(),
            guides: snapshot.guides().to_vec(),
            grid: snapshot.grid(),
            tiles: snapshot
                .tiles()
                .iter()
                .map(|tile| TileObservation {
                    semantic: SemanticTileObservation {
                        tile_id: tile.tile_id(),
                        origin_x: tile.origin_x(),
                        origin_y: tile.origin_y(),
                        width: tile.width(),
                        height: tile.height(),
                        stride_bytes: tile.stride_bytes(),
                        pixel_checksum: fnv1a64(tile.pixels()),
                    },
                    tile_revision: tile.tile_revision(),
                })
                .collect(),
        };
        let features = FeatureObservation {
            selection_bounds: core.selection_bounds()?,
            palette: core.palette()?.to_vec(),
            main_line_color: core.main_line_color()?,
            guides: core.guides()?.to_vec(),
            grid: core.grid()?,
            light_table_sets: core.light_table_sets()?,
            light_table_items: core.light_table_items()?,
        };
        Ok(Self {
            common: CommonObservation {
                document,
                topology,
                history,
                main_view,
                snapshot,
            },
            features,
        })
    }

    fn document_semantics(&self) -> DocumentSemanticObservation {
        let info = self.common.document;
        DocumentSemanticObservation {
            document_id: info.document_id,
            document_uuid: info.document_uuid,
            width: info.width,
            height: info.height,
            dpi_x_milli: info.dpi_x_milli,
            dpi_y_milli: info.dpi_y_milli,
            frames: info.frames,
            main_plane_checksum: info.main_plane_checksum,
            color_plane_checksum: info.color_plane_checksum,
            topology: self.common.topology.clone(),
            snapshot_tiles: self
                .common
                .snapshot
                .tiles
                .iter()
                .map(|tile| tile.semantic.clone())
                .collect(),
            features: self.features.clone(),
        }
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResultClass {
    Success,
    NoOp,
    Invalid,
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MutationKind {
    Document,
    View,
    History,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ResultValue {
    None,
    Id(u64),
    Count(u64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExecutionResult {
    class: ResultClass,
    value: ResultValue,
}

impl ExecutionResult {
    fn dispatch(result: Result<DispatchOutcome, CoreError>) -> Self {
        match result {
            Ok(outcome) if outcome.accepted_commands() == 0 => Self {
                class: ResultClass::NoOp,
                value: ResultValue::None,
            },
            Ok(_) => Self {
                class: ResultClass::Success,
                value: ResultValue::None,
            },
            Err(CoreError::Cancelled) => Self {
                class: ResultClass::Cancel,
                value: ResultValue::None,
            },
            Err(_) => Self {
                class: ResultClass::Invalid,
                value: ResultValue::None,
            },
        }
    }

    fn created(result: Result<(DispatchOutcome, u64), CoreError>) -> Self {
        match result {
            Ok((_, id)) => Self {
                class: ResultClass::Success,
                value: ResultValue::Id(id),
            },
            Err(CoreError::Cancelled) => Self {
                class: ResultClass::Cancel,
                value: ResultValue::None,
            },
            Err(_) => Self {
                class: ResultClass::Invalid,
                value: ResultValue::None,
            },
        }
    }

    fn dispatch_class(result: Result<DispatchOutcome, CoreError>, class: ResultClass) -> Self {
        match result {
            Ok(_) => Self {
                class,
                value: ResultValue::None,
            },
            Err(CoreError::Cancelled) => Self {
                class: ResultClass::Cancel,
                value: ResultValue::None,
            },
            Err(_) => Self {
                class: ResultClass::Invalid,
                value: ResultValue::None,
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ActionSpec {
    domain: u8,
    opcode: u8,
    a: u8,
    b: i8,
}

fn action_strategy() -> impl Strategy<Value = Vec<ActionSpec>> {
    proptest::collection::vec(
        (0_u8..6, any::<u8>(), any::<u8>(), any::<i8>()).prop_map(|(domain, opcode, a, b)| {
            ActionSpec {
                domain,
                opcode,
                a,
                b,
            }
        }),
        8..20,
    )
}

#[derive(Clone, Debug)]
enum ConcreteAction {
    CreateRasterLayer(String),
    DuplicateLayer(u64),
    ReorderLayer(u64, usize),
    SetLayer(u64, bool, bool, u32, String, ResultClass),
    DeleteLayer(u64),
    CreateRasterPlane(u64, String),
    DuplicatePlane(u64),
    ReorderPlane(u64, usize),
    SetPlane(u64, bool, bool, u32, String, ResultClass),
    DeletePlane(u64),
    InvalidDeleteLayer,
    AddGuide(GuideAxis, i32),
    MoveGuide(u64, i32),
    DeleteGuide(u64),
    SetGrid(GridConfig),
    InvalidGuide,
    Pan(f64, f64, ResultClass),
    Flip(MirrorAxis),
    SetGridVisible(bool),
    InvalidView,
    CreateView,
    ApplySecondaryView(u64, f64),
    CloseView(u64),
    InvalidCloseView,
    SelectRect(RectI32, SelectionOperation),
    InvertSelection,
    ClearSelection(ResultClass),
    ResizeSelection(i32),
    InvalidSelection,
    Stroke(u32, u32, [u8; 4]),
    Fill(u32, u32, [u8; 4]),
    InvalidFill,
    CancelFill,
    CreateLightSet(String),
    RenameLightSet(u64, String),
    SetActiveLightSet(u64),
    AddLightItem(u8),
    SetLightOpacity(u32),
    InvalidLightSet,
    Undo,
    Redo,
    JumpHistory(usize),
    InvalidHistoryJump,
}

impl ConcreteAction {
    fn expected_class(&self) -> Option<ResultClass> {
        match self {
            Self::SelectRect(_, _) | Self::Stroke(_, _, _) | Self::Fill(_, _, _) => None,
            Self::InvalidDeleteLayer
            | Self::InvalidGuide
            | Self::InvalidView
            | Self::InvalidCloseView
            | Self::InvalidSelection
            | Self::InvalidFill
            | Self::InvalidLightSet
            | Self::InvalidHistoryJump => Some(ResultClass::Invalid),
            Self::CancelFill => Some(ResultClass::Cancel),
            Self::SetLayer(_, _, _, _, _, class)
            | Self::SetPlane(_, _, _, _, _, class)
            | Self::Pan(_, _, class)
            | Self::ClearSelection(class) => Some(*class),
            Self::MoveGuide(_, _)
            | Self::SetGrid(_)
            | Self::SetGridVisible(_)
            | Self::ResizeSelection(_)
            | Self::RenameLightSet(_, _)
            | Self::SetActiveLightSet(_)
            | Self::SetLightOpacity(_)
            | Self::JumpHistory(_) => Some(ResultClass::NoOp),
            _ => Some(ResultClass::Success),
        }
    }

    fn mutation_kind(&self) -> MutationKind {
        match self {
            Self::Pan(_, _, _)
            | Self::Flip(_)
            | Self::SetGridVisible(_)
            | Self::InvalidView
            | Self::CreateView
            | Self::ApplySecondaryView(_, _)
            | Self::CloseView(_)
            | Self::InvalidCloseView => MutationKind::View,
            Self::Undo | Self::Redo | Self::JumpHistory(_) | Self::InvalidHistoryJump => {
                MutationKind::History
            }
            _ => MutationKind::Document,
        }
    }

    fn execute(&self, core: &mut Core) -> ExecutionResult {
        match self {
            Self::CreateRasterLayer(name) => ExecutionResult::created(core.create_layer(name)),
            Self::DuplicateLayer(id) => ExecutionResult::created(core.duplicate_layer(*id)),
            Self::ReorderLayer(id, destination) => {
                ExecutionResult::dispatch(core.reorder_layer(*id, *destination))
            }
            Self::SetLayer(id, visible, editable, opacity, name, class) => {
                ExecutionResult::dispatch_class(
                    core.set_layer_properties(*id, *visible, *editable, *opacity, name),
                    *class,
                )
            }
            Self::DeleteLayer(id) => ExecutionResult::dispatch(core.delete_layer(*id)),
            Self::CreateRasterPlane(layer, name) => ExecutionResult::created(core.create_plane(
                *layer,
                PixelFormat::StraightRgba8,
                name,
            )),
            Self::DuplicatePlane(id) => ExecutionResult::created(core.duplicate_plane(*id)),
            Self::ReorderPlane(id, destination) => {
                ExecutionResult::dispatch(core.reorder_plane(*id, *destination))
            }
            Self::SetPlane(id, visible, editable, opacity, name, class) => {
                ExecutionResult::dispatch_class(
                    core.set_plane_properties(*id, *visible, *editable, *opacity, name),
                    *class,
                )
            }
            Self::DeletePlane(id) => ExecutionResult::dispatch(core.delete_plane(*id)),
            Self::InvalidDeleteLayer => ExecutionResult::dispatch(core.delete_layer(u64::MAX)),
            Self::AddGuide(axis, position) => {
                ExecutionResult::created(core.add_guide(*axis, *position))
            }
            Self::MoveGuide(id, position) => {
                ExecutionResult::dispatch_class(core.move_guide(*id, *position), ResultClass::NoOp)
            }
            Self::DeleteGuide(id) => ExecutionResult::dispatch(core.delete_guide(*id)),
            Self::SetGrid(grid) => {
                ExecutionResult::dispatch_class(core.set_grid(*grid), ResultClass::NoOp)
            }
            Self::InvalidGuide => {
                ExecutionResult::created(core.add_guide(GuideAxis::Horizontal, -1))
            }
            Self::Pan(dx, dy, _) => {
                let before = core.view_state().revision();
                match core.apply_view(ViewCommand::PanBy {
                    device_dx: *dx,
                    device_dy: *dy,
                }) {
                    Ok(view) => ExecutionResult {
                        class: if view.revision() == before {
                            ResultClass::NoOp
                        } else {
                            ResultClass::Success
                        },
                        value: ResultValue::None,
                    },
                    Err(_) => ExecutionResult {
                        class: ResultClass::Invalid,
                        value: ResultValue::None,
                    },
                }
            }
            Self::Flip(axis) => match core.apply_view(ViewCommand::Flip { axis: *axis }) {
                Ok(_) => ExecutionResult {
                    class: ResultClass::Success,
                    value: ResultValue::None,
                },
                Err(_) => ExecutionResult {
                    class: ResultClass::Invalid,
                    value: ResultValue::None,
                },
            },
            Self::SetGridVisible(visible) => {
                let before = core.view_state().revision();
                match core.apply_view(ViewCommand::SetGridVisible(*visible)) {
                    Ok(view) => ExecutionResult {
                        class: if view.revision() == before {
                            ResultClass::NoOp
                        } else {
                            ResultClass::Success
                        },
                        value: ResultValue::None,
                    },
                    Err(_) => ExecutionResult {
                        class: ResultClass::Invalid,
                        value: ResultValue::None,
                    },
                }
            }
            Self::InvalidView => match core.apply_view(ViewCommand::ZoomAt {
                factor: 0.0,
                device_x: 0.0,
                device_y: 0.0,
            }) {
                Ok(_) => ExecutionResult {
                    class: ResultClass::Success,
                    value: ResultValue::None,
                },
                Err(_) => ExecutionResult {
                    class: ResultClass::Invalid,
                    value: ResultValue::None,
                },
            },
            Self::CreateView => match core.create_view() {
                Ok(id) => ExecutionResult {
                    class: ResultClass::Success,
                    value: ResultValue::Id(id),
                },
                Err(_) => ExecutionResult {
                    class: ResultClass::Invalid,
                    value: ResultValue::None,
                },
            },
            Self::ApplySecondaryView(id, dx) => {
                match core.apply_view_for(
                    *id,
                    ViewCommand::PanBy {
                        device_dx: *dx,
                        device_dy: 0.0,
                    },
                ) {
                    Ok(_) => ExecutionResult {
                        class: ResultClass::Success,
                        value: ResultValue::None,
                    },
                    Err(_) => ExecutionResult {
                        class: ResultClass::Invalid,
                        value: ResultValue::None,
                    },
                }
            }
            Self::CloseView(id) => match core.close_view(*id) {
                Ok(()) => ExecutionResult {
                    class: ResultClass::Success,
                    value: ResultValue::None,
                },
                Err(_) => ExecutionResult {
                    class: ResultClass::Invalid,
                    value: ResultValue::None,
                },
            },
            Self::InvalidCloseView => match core.close_view(u64::MAX) {
                Ok(()) => ExecutionResult {
                    class: ResultClass::Success,
                    value: ResultValue::None,
                },
                Err(_) => ExecutionResult {
                    class: ResultClass::Invalid,
                    value: ResultValue::None,
                },
            },
            Self::SelectRect(bounds, operation) => {
                let before = core.document_info().map(|info| info.document_revision);
                match (
                    before,
                    core.apply_selection(&SelectionShape::Rectangle(*bounds), *operation),
                ) {
                    (Ok(before), Ok(_)) => ExecutionResult {
                        class: if core
                            .document_info()
                            .is_ok_and(|info| info.document_revision == before)
                        {
                            ResultClass::NoOp
                        } else {
                            ResultClass::Success
                        },
                        value: ResultValue::None,
                    },
                    (_, Err(CoreError::Cancelled)) => ExecutionResult {
                        class: ResultClass::Cancel,
                        value: ResultValue::None,
                    },
                    _ => ExecutionResult {
                        class: ResultClass::Invalid,
                        value: ResultValue::None,
                    },
                }
            }
            Self::InvertSelection => ExecutionResult::dispatch(core.invert_selection()),
            Self::ClearSelection(class) => {
                ExecutionResult::dispatch_class(core.clear_selection(), *class)
            }
            Self::ResizeSelection(pixels) => {
                ExecutionResult::dispatch_class(core.resize_selection(*pixels), ResultClass::NoOp)
            }
            Self::InvalidSelection => ExecutionResult::dispatch(core.resize_selection(i32::MIN)),
            Self::Stroke(x, y, color) => {
                if core.set_active_plane(ActivePlane::Color).is_err() {
                    return ExecutionResult {
                        class: ResultClass::Invalid,
                        value: ResultValue::None,
                    };
                }
                let before = core.document_info().map(|info| info.document_revision);
                match (
                    before,
                    core.apply_stroke(&Stroke {
                        tool: PaintTool::Pencil,
                        plane: ActivePlane::Color,
                        color: *color,
                        diameter: 1.0,
                        shape: BrushShape::Round,
                        smoothing: 0,
                        start_color: StartColorPredicate::Any,
                        auto_erase: false,
                        pressure_size: false,
                        coordinate_space: CoordinateSpace::Document,
                        samples: vec![StrokeSample {
                            x: *x as f32,
                            y: *y as f32,
                            pressure: 1.0,
                        }],
                    }),
                ) {
                    (Ok(before), Ok(_)) => ExecutionResult {
                        class: if core
                            .document_info()
                            .is_ok_and(|info| info.document_revision == before)
                        {
                            ResultClass::NoOp
                        } else {
                            ResultClass::Success
                        },
                        value: ResultValue::None,
                    },
                    (_, Err(CoreError::Cancelled)) => ExecutionResult {
                        class: ResultClass::Cancel,
                        value: ResultValue::None,
                    },
                    _ => ExecutionResult {
                        class: ResultClass::Invalid,
                        value: ResultValue::None,
                    },
                }
            }
            Self::Fill(x, y, color) => {
                if core.set_active_plane(ActivePlane::Color).is_err() {
                    return ExecutionResult {
                        class: ResultClass::Invalid,
                        value: ResultValue::None,
                    };
                }
                match core.apply_fill(&fill_request(*x, *y, *color)) {
                    Ok(outcome) => ExecutionResult {
                        class: if outcome.changed_pixels == 0 {
                            ResultClass::NoOp
                        } else {
                            ResultClass::Success
                        },
                        value: ResultValue::Count(outcome.changed_pixels),
                    },
                    Err(CoreError::Cancelled) => ExecutionResult {
                        class: ResultClass::Cancel,
                        value: ResultValue::None,
                    },
                    Err(_) => ExecutionResult {
                        class: ResultClass::Invalid,
                        value: ResultValue::None,
                    },
                }
            }
            Self::InvalidFill => match core.apply_fill(&fill_request(16, 16, [1, 2, 3, 255])) {
                Ok(outcome) => ExecutionResult {
                    class: ResultClass::Success,
                    value: ResultValue::Count(outcome.changed_pixels),
                },
                Err(_) => ExecutionResult {
                    class: ResultClass::Invalid,
                    value: ResultValue::None,
                },
            },
            Self::CancelFill => {
                match core.apply_fill_with_cancel(&fill_request(0, 0, [9, 8, 7, 255]), || true) {
                    Ok(outcome) => ExecutionResult {
                        class: ResultClass::Success,
                        value: ResultValue::Count(outcome.changed_pixels),
                    },
                    Err(CoreError::Cancelled) => ExecutionResult {
                        class: ResultClass::Cancel,
                        value: ResultValue::None,
                    },
                    Err(_) => ExecutionResult {
                        class: ResultClass::Invalid,
                        value: ResultValue::None,
                    },
                }
            }
            Self::CreateLightSet(name) => {
                ExecutionResult::created(core.light_table_create_set(name.clone()))
            }
            Self::RenameLightSet(id, name) => ExecutionResult::dispatch_class(
                core.light_table_rename_set(*id, name.clone()),
                ResultClass::NoOp,
            ),
            Self::SetActiveLightSet(id) => {
                ExecutionResult::dispatch_class(core.light_table_set_active(*id), ResultClass::NoOp)
            }
            Self::AddLightItem(salt) => {
                let source = LightTableSource::from_rgba_bytes(
                    0x4c49_4748_5454_4142_4c45_u128 + u128::from(*salt),
                    1,
                    RectI32 {
                        x: 0,
                        y: 0,
                        width: 2,
                        height: 2,
                    },
                    RgbaRasterBytes {
                        width: 2,
                        height: 2,
                        pixel_format: PixelFormat::StraightRgba8,
                        dpi_x_milli: Some(DEFAULT_DPI_MILLI),
                        dpi_y_milli: Some(DEFAULT_DPI_MILLI),
                        pixels: [*salt, 20, 30, 255].repeat(4),
                    },
                );
                match source {
                    Ok(source) => ExecutionResult::created(core.light_table_add_item(
                        LightTableItemInput::new(format!("Reference {salt}"), source),
                    )),
                    Err(_) => ExecutionResult {
                        class: ResultClass::Invalid,
                        value: ResultValue::None,
                    },
                }
            }
            Self::SetLightOpacity(opacity) => ExecutionResult::dispatch_class(
                core.light_table_set_global_opacity(*opacity),
                ResultClass::NoOp,
            ),
            Self::InvalidLightSet => {
                ExecutionResult::dispatch(core.light_table_set_active(u64::MAX))
            }
            Self::Undo => ExecutionResult::dispatch(core.undo()),
            Self::Redo => ExecutionResult::dispatch(core.redo()),
            Self::JumpHistory(cursor) => {
                ExecutionResult::dispatch_class(core.jump_history(*cursor), ResultClass::NoOp)
            }
            Self::InvalidHistoryJump => ExecutionResult::dispatch(core.jump_history(usize::MAX)),
        }
    }
}

#[derive(Default)]
struct AbstractState {
    view_ids: Vec<u64>,
}

impl AbstractState {
    fn update(&mut self, action: &ConcreteAction, result: &ExecutionResult) {
        match (action, &result.value) {
            (ConcreteAction::CreateView, ResultValue::Id(id)) => self.view_ids.push(*id),
            (ConcreteAction::CloseView(id), _) if result.class == ResultClass::Success => {
                self.view_ids.retain(|candidate| candidate != id);
            }
            _ => {}
        }
    }
}

fn resolve_action(
    core: &Core,
    state: &AbstractState,
    spec: ActionSpec,
) -> Result<ConcreteAction, CoreError> {
    let layers = core.layers()?;
    let standard_layers: Vec<_> = layers.iter().collect();
    let choose = |length: usize| usize::from(spec.a) % length.max(1);
    match spec.domain {
        0 => match spec.opcode % 13 {
            0 => Ok(ConcreteAction::CreateRasterLayer(format!(
                "Raster {}",
                spec.a
            ))),
            1 => Ok(standard_layers.first().map_or(
                ConcreteAction::CreateRasterLayer(format!("Raster {}", spec.a)),
                |layer| ConcreteAction::DuplicateLayer(layer.id),
            )),
            2 if layers.len() > 1 => {
                let source = choose(layers.len());
                let destination = (source + 1) % layers.len();
                Ok(ConcreteAction::ReorderLayer(layers[source].id, destination))
            }
            2 => Ok(ConcreteAction::SetLayer(
                layers[0].id,
                layers[0].visible,
                layers[0].editable,
                layers[0].opacity_milli,
                layers[0].name.clone(),
                ResultClass::NoOp,
            )),
            3 => {
                let layer = &layers[choose(layers.len())];
                Ok(ConcreteAction::SetLayer(
                    layer.id,
                    !layer.visible,
                    layer.editable,
                    layer.opacity_milli,
                    layer.name.clone(),
                    ResultClass::Success,
                ))
            }
            4 => {
                let layer = &layers[choose(layers.len())];
                Ok(ConcreteAction::SetLayer(
                    layer.id,
                    layer.visible,
                    layer.editable,
                    layer.opacity_milli,
                    layer.name.clone(),
                    ResultClass::NoOp,
                ))
            }
            5 if standard_layers.len() > 1 => Ok(ConcreteAction::DeleteLayer(
                standard_layers.last().unwrap().id,
            )),
            5 => Ok(ConcreteAction::InvalidDeleteLayer),
            6 => Ok(standard_layers.first().map_or(
                ConcreteAction::CreateRasterLayer(format!("Raster {}", spec.a)),
                |layer| ConcreteAction::CreateRasterPlane(layer.id, format!("Plane {}", spec.a)),
            )),
            7 => Ok(standard_layers
                .iter()
                .find_map(|layer| {
                    layer
                        .planes
                        .iter()
                        .find(|plane| plane.kind == PlaneType::Raster)
                })
                .map_or_else(
                    || {
                        standard_layers.first().map_or(
                            ConcreteAction::InvalidDeleteLayer,
                            |layer| {
                                ConcreteAction::CreateRasterPlane(
                                    layer.id,
                                    format!("Plane {}", spec.a),
                                )
                            },
                        )
                    },
                    |plane| ConcreteAction::DuplicatePlane(plane.id),
                )),
            8 => Ok(standard_layers
                .iter()
                .find(|layer| layer.planes.len() > 1)
                .map_or(ConcreteAction::InvalidDeleteLayer, |layer| {
                    ConcreteAction::ReorderPlane(layer.planes[0].id, 1)
                })),
            9 => Ok(standard_layers
                .first()
                .and_then(|layer| layer.planes.first())
                .map_or(ConcreteAction::InvalidDeleteLayer, |plane| {
                    ConcreteAction::SetPlane(
                        plane.id,
                        !plane.visible,
                        plane.editable,
                        plane.opacity_milli,
                        plane.name.clone(),
                        ResultClass::Success,
                    )
                })),
            10 => Ok(standard_layers
                .first()
                .and_then(|layer| layer.planes.first())
                .map_or(ConcreteAction::InvalidDeleteLayer, |plane| {
                    ConcreteAction::SetPlane(
                        plane.id,
                        plane.visible,
                        plane.editable,
                        plane.opacity_milli,
                        plane.name.clone(),
                        ResultClass::NoOp,
                    )
                })),
            11 => Ok(standard_layers
                .iter()
                .find_map(|layer| {
                    layer
                        .planes
                        .iter()
                        .rev()
                        .find(|plane| plane.kind == PlaneType::Raster)
                })
                .map_or(ConcreteAction::InvalidDeleteLayer, |plane| {
                    ConcreteAction::DeletePlane(plane.id)
                })),
            _ => Ok(ConcreteAction::InvalidDeleteLayer),
        },
        1 => match spec.opcode % 13 {
            0 => Ok(ConcreteAction::AddGuide(
                if spec.a % 2 == 0 {
                    GuideAxis::Horizontal
                } else {
                    GuideAxis::Vertical
                },
                i32::from(spec.a % 17),
            )),
            1 => Ok(core
                .guides()?
                .first()
                .map_or(ConcreteAction::InvalidGuide, |guide| {
                    ConcreteAction::MoveGuide(guide.id, guide.position)
                })),
            2 => Ok(core
                .guides()?
                .first()
                .map_or(ConcreteAction::InvalidGuide, |guide| {
                    ConcreteAction::DeleteGuide(guide.id)
                })),
            3 => Ok(ConcreteAction::SetGrid(core.grid()?)),
            4 => Ok(ConcreteAction::InvalidGuide),
            5 => Ok(ConcreteAction::Pan(
                f64::from(spec.b),
                1.0,
                ResultClass::Success,
            )),
            6 => Ok(ConcreteAction::Pan(0.0, 0.0, ResultClass::NoOp)),
            7 => Ok(ConcreteAction::Flip(if spec.a % 2 == 0 {
                MirrorAxis::Horizontal
            } else {
                MirrorAxis::Vertical
            })),
            8 => Ok(ConcreteAction::SetGridVisible(
                core.view_state().grid_visible(),
            )),
            9 => Ok(ConcreteAction::InvalidView),
            10 => Ok(ConcreteAction::CreateView),
            11 => Ok(state
                .view_ids
                .first()
                .map_or(ConcreteAction::CreateView, |id| {
                    ConcreteAction::ApplySecondaryView(*id, 1.0)
                })),
            12 => Ok(state
                .view_ids
                .first()
                .map_or(ConcreteAction::InvalidCloseView, |id| {
                    ConcreteAction::CloseView(*id)
                })),
            _ => unreachable!(),
        },
        2 => match spec.opcode % 6 {
            0 => {
                let bounds = RectI32 {
                    x: i32::from(spec.a % 8),
                    y: i32::from(spec.a.wrapping_mul(3) % 8),
                    width: 4,
                    height: 4,
                };
                Ok(ConcreteAction::SelectRect(bounds, SelectionOperation::New))
            }
            1 => Ok(ConcreteAction::InvertSelection),
            2 => Ok(ConcreteAction::ClearSelection(
                if core.selection_bounds()?.is_some() {
                    ResultClass::Success
                } else {
                    ResultClass::NoOp
                },
            )),
            3 => Ok(ConcreteAction::ResizeSelection(0)),
            4 => Ok(ConcreteAction::InvalidSelection),
            _ => {
                let bounds = RectI32 {
                    x: 0,
                    y: 0,
                    width: 16,
                    height: 16,
                };
                Ok(ConcreteAction::SelectRect(bounds, SelectionOperation::Add))
            }
        },
        3 => {
            let x = u32::from(spec.a % 16);
            let y = u32::from(spec.a.wrapping_mul(7) % 16);
            let current = match core.plane_pixel(ActivePlane::Color, x, y)? {
                PixelValue::Rgba(value) => value,
                PixelValue::Rgba16(value) => value.map(|channel| (channel / 257) as u8),
                _ => [0; 4],
            };
            let color = [current[0] ^ 0xff, spec.a, spec.a ^ 0x55, 255];
            match spec.opcode % 4 {
                0 => Ok(ConcreteAction::Stroke(x, y, color)),
                1 => Ok(ConcreteAction::Fill(x, y, color)),
                2 => Ok(ConcreteAction::InvalidFill),
                _ => Ok(ConcreteAction::CancelFill),
            }
        }
        4 => {
            let sets = core.light_table_sets()?;
            let active = sets.iter().find(|set| set.active).unwrap();
            match spec.opcode % 6 {
                0 => Ok(ConcreteAction::CreateLightSet(format!("Set {}", spec.a))),
                1 => Ok(ConcreteAction::RenameLightSet(
                    active.id,
                    active.name.clone(),
                )),
                2 => Ok(ConcreteAction::SetActiveLightSet(active.id)),
                3 => Ok(ConcreteAction::AddLightItem(spec.a)),
                4 => Ok(ConcreteAction::SetLightOpacity(active.global_opacity_milli)),
                _ => Ok(ConcreteAction::InvalidLightSet),
            }
        }
        _ => {
            let cursor = core.history_cursor();
            let history_len = core.history_entries().len();
            match spec.opcode % 4 {
                0 if cursor > 0 => Ok(ConcreteAction::Undo),
                0 => Ok(ConcreteAction::JumpHistory(cursor)),
                1 if cursor < history_len => Ok(ConcreteAction::Redo),
                1 => Ok(ConcreteAction::JumpHistory(cursor)),
                2 => Ok(ConcreteAction::JumpHistory(cursor)),
                _ => Ok(ConcreteAction::InvalidHistoryJump),
            }
        }
    }
}

fn mandatory_specs() -> Vec<ActionSpec> {
    let mut specs = Vec::new();
    for opcode in 0..13 {
        specs.push(ActionSpec {
            domain: 0,
            opcode,
            a: 1,
            b: 1,
        });
    }
    for opcode in 0..13 {
        specs.push(ActionSpec {
            domain: 1,
            opcode,
            a: 4,
            b: 2,
        });
    }
    for opcode in 0..6 {
        specs.push(ActionSpec {
            domain: 2,
            opcode,
            a: 2,
            b: 0,
        });
    }
    for opcode in 0..4 {
        specs.push(ActionSpec {
            domain: 3,
            opcode,
            a: 3,
            b: 0,
        });
    }
    for opcode in 0..6 {
        specs.push(ActionSpec {
            domain: 4,
            opcode,
            a: 6,
            b: 0,
        });
    }
    for opcode in 0..4 {
        specs.push(ActionSpec {
            domain: 5,
            opcode,
            a: 0,
            b: 0,
        });
    }
    specs
}

fn seeded_core() -> Core {
    let mut core = Core::new();
    core.new_cell_with_uuid(
        16,
        16,
        DEFAULT_DPI_MILLI,
        DEFAULT_DPI_MILLI,
        STATE_MACHINE_UUID,
    )
    .unwrap();
    core
}

fn replay(seed: u64, case: usize, step: usize, actions: &[ActionSpec], detail: &str) -> String {
    format!("seed={seed:#018x} case={case} step={step} actions={actions:?}: {detail}")
}

fn fail(
    seed: u64,
    case: usize,
    step: usize,
    actions: &[ActionSpec],
    detail: impl Into<String>,
) -> TestCaseError {
    TestCaseError::fail(replay(seed, case, step, actions, &detail.into()))
}

fn assert_id_integrity(
    observation: &CoreObservation,
    seed: u64,
    case: usize,
    step: usize,
    actions: &[ActionSpec],
) -> Result<(), TestCaseError> {
    let mut ids = BTreeSet::new();
    for layer in &observation.common.topology {
        if !ids.insert(layer.id) {
            return Err(fail(seed, case, step, actions, "duplicate layer ID"));
        }
        for plane in &layer.planes {
            if !ids.insert(plane.id) {
                return Err(fail(seed, case, step, actions, "duplicate plane ID"));
            }
        }
    }
    Ok(())
}

fn assert_view_separation(
    before: &CoreObservation,
    after: &CoreObservation,
    seed: u64,
    case: usize,
    step: usize,
    actions: &[ActionSpec],
) -> Result<(), TestCaseError> {
    let left = before.common.document;
    let right = after.common.document;
    if left.document_revision != right.document_revision
        || left.dirty != right.dirty
        || before.common.history != after.common.history
        || left.main_plane_checksum != right.main_plane_checksum
        || left.color_plane_checksum != right.color_plane_checksum
    {
        return Err(fail(
            seed,
            case,
            step,
            actions,
            "view-only operation changed document revision, history, dirty state, or pixels",
        ));
    }
    Ok(())
}

fn assert_document_round_trip(
    core: &mut Core,
    before: &CoreObservation,
    after: &CoreObservation,
    seed: u64,
    case: usize,
    step: usize,
    actions: &[ActionSpec],
) -> Result<(), TestCaseError> {
    if after.common.history.cursor != before.common.history.cursor + 1 {
        return Err(fail(
            seed,
            case,
            step,
            actions,
            format!(
                "successful document edit was not exactly one history unit: before={} after={}",
                before.common.history.cursor, after.common.history.cursor
            ),
        ));
    }
    core.undo()
        .map_err(|error| fail(seed, case, step, actions, format!("undo failed: {error}")))?;
    let undone = CoreObservation::capture(core).map_err(|error| {
        fail(
            seed,
            case,
            step,
            actions,
            format!("undo observation failed: {error}"),
        )
    })?;
    if undone.document_semantics() != before.document_semantics() {
        return Err(fail(
            seed,
            case,
            step,
            actions,
            "one Undo did not restore the preceding document semantics",
        ));
    }
    core.redo()
        .map_err(|error| fail(seed, case, step, actions, format!("redo failed: {error}")))?;
    let redone = CoreObservation::capture(core).map_err(|error| {
        fail(
            seed,
            case,
            step,
            actions,
            format!("redo observation failed: {error}"),
        )
    })?;
    if redone.document_semantics() != after.document_semantics() {
        return Err(fail(
            seed,
            case,
            step,
            actions,
            "one Redo did not restore the edited document semantics",
        ));
    }
    Ok(())
}

fn run_sequence(seed: u64, case: usize, generated: &[ActionSpec]) -> Result<(), TestCaseError> {
    let mut actions = mandatory_specs();
    actions.extend_from_slice(generated);
    let mut left = seeded_core();
    let mut right = seeded_core();
    let mut model = AbstractState::default();
    let initial_left = CoreObservation::capture(&mut left).unwrap();
    let initial_right = CoreObservation::capture(&mut right).unwrap();
    if initial_left != initial_right {
        return Err(fail(seed, case, 0, &actions, "initial observations differ"));
    }

    let mut seen = BTreeSet::new();
    for (step, spec) in actions.iter().copied().enumerate() {
        let action = resolve_action(&left, &model, spec).map_err(|error| {
            fail(
                seed,
                case,
                step,
                &actions,
                format!("operation resolution failed: {error}"),
            )
        })?;
        let before_left = CoreObservation::capture(&mut left).map_err(|error| {
            fail(
                seed,
                case,
                step,
                &actions,
                format!("left pre-observation failed: {error}"),
            )
        })?;
        let before_right = CoreObservation::capture(&mut right).map_err(|error| {
            fail(
                seed,
                case,
                step,
                &actions,
                format!("right pre-observation failed: {error}"),
            )
        })?;
        if before_left != before_right {
            return Err(fail(
                seed,
                case,
                step,
                &actions,
                "cores diverged before operation",
            ));
        }

        let left_result = action.execute(&mut left);
        let right_result = action.execute(&mut right);
        if left_result != right_result {
            return Err(fail(
                seed,
                case,
                step,
                &actions,
                format!(
                    "result mismatch for {action:?}: left={left_result:?} right={right_result:?}"
                ),
            ));
        }
        if let Some(expected) = action.expected_class() {
            if left_result.class != expected {
                return Err(fail(
                    seed,
                    case,
                    step,
                    &actions,
                    format!(
                        "result class mismatch for {action:?}: expected={expected:?} actual={:?}",
                        left_result.class
                    ),
                ));
            }
        }
        seen.insert(left_result.class as u8);
        let after_left = CoreObservation::capture(&mut left).map_err(|error| {
            fail(
                seed,
                case,
                step,
                &actions,
                format!("left post-observation failed: {error}"),
            )
        })?;
        let after_right = CoreObservation::capture(&mut right).map_err(|error| {
            fail(
                seed,
                case,
                step,
                &actions,
                format!("right post-observation failed: {error}"),
            )
        })?;
        if after_left != after_right {
            return Err(fail(
                seed,
                case,
                step,
                &actions,
                format!("observation mismatch after {action:?}"),
            ));
        }
        if matches!(
            left_result.class,
            ResultClass::NoOp | ResultClass::Invalid | ResultClass::Cancel
        ) && (before_left != after_left || before_right != after_right)
        {
            return Err(fail(
                seed,
                case,
                step,
                &actions,
                format!("atomic {action:?} changed observable state"),
            ));
        }
        if action.mutation_kind() == MutationKind::View {
            assert_view_separation(&before_left, &after_left, seed, case, step, &actions)?;
            assert_view_separation(&before_right, &after_right, seed, case, step, &actions)?;
        }
        if action.mutation_kind() == MutationKind::Document
            && left_result.class == ResultClass::Success
        {
            assert_document_round_trip(
                &mut left,
                &before_left,
                &after_left,
                seed,
                case,
                step,
                &actions,
            )?;
            assert_document_round_trip(
                &mut right,
                &before_right,
                &after_right,
                seed,
                case,
                step,
                &actions,
            )?;
        }
        let settled_left = CoreObservation::capture(&mut left).unwrap();
        let settled_right = CoreObservation::capture(&mut right).unwrap();
        if settled_left != settled_right {
            return Err(fail(
                seed,
                case,
                step,
                &actions,
                "cores diverged after contract checks",
            ));
        }
        assert_id_integrity(&settled_left, seed, case, step, &actions)?;
        model.update(&action, &left_result);
    }
    for class in [
        ResultClass::Success,
        ResultClass::NoOp,
        ResultClass::Invalid,
        ResultClass::Cancel,
    ] {
        if !seen.contains(&(class as u8)) {
            return Err(fail(
                seed,
                case,
                actions.len(),
                &actions,
                format!("sequence did not cover {class:?}"),
            ));
        }
    }
    Ok(())
}

fn case_seed(seed: u64, case: usize) -> [u8; 32] {
    let mut bytes = [0_u8; 32];
    for (index, chunk) in bytes.chunks_exact_mut(8).enumerate() {
        let value = seed
            .rotate_left((index * 11) as u32)
            .wrapping_add((case as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15))
            .wrapping_add(index as u64);
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

#[test]
fn fixed_seed_state_machine_is_deterministic_atomic_and_replayable() {
    for seed in STATE_MACHINE_SEEDS {
        for case in 0..3 {
            let config = Config {
                cases: 1,
                failure_persistence: None,
                max_shrink_iters: 4_096,
                ..Config::default()
            };
            let rng = TestRng::from_seed(RngAlgorithm::ChaCha, &case_seed(seed, case));
            let mut runner = TestRunner::new_with_rng(config, rng);
            if let Err(error) = runner.run(&action_strategy(), |actions| {
                run_sequence(seed, case, &actions)
            }) {
                panic!("state-machine replay seed={seed:#018x} case={case}: {error}");
            }
        }
    }
}

#[test]
fn cancel_sessions_and_cancellable_fill_restore_the_common_observation() {
    let mut core = seeded_core();
    let base = CoreObservation::capture(&mut core).unwrap();
    core.begin_stroke(&line_stroke(vec![StrokeSample {
        x: 3.0,
        y: 3.0,
        pressure: 1.0,
    }]))
    .unwrap();
    core.append_stroke(&[StrokeSample {
        x: 8.0,
        y: 8.0,
        pressure: 1.0,
    }])
    .unwrap();
    core.cancel_stroke();
    assert_eq!(CoreObservation::capture(&mut core).unwrap(), base);

    let color_plane_id = core.document_info().unwrap().color_plane_id;
    core.begin_filter_preview(
        color_plane_id,
        Filter::Invert {
            channel: Channel::Rgb,
        },
    )
    .unwrap();
    core.cancel_filter_preview().unwrap();
    assert_eq!(CoreObservation::capture(&mut core).unwrap(), base);

    let payload = ClipboardPayload {
        source_document_uuid: 7,
        bounds: RectI32 {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        },
        planes: vec![ClipboardPlane {
            kind: PlaneType::Color,
            pixel_format: PixelFormat::StraightRgba8,
            origin_x: 0,
            origin_y: 0,
            pixels: vec![ClipboardPixel {
                x: 0,
                y: 0,
                value: PixelValue::Rgba([1, 2, 3, 255]),
            }],
        }],
    };
    core.begin_paste(&payload).unwrap();
    core.set_floating_transform(FloatingTransform {
        target_x: 2.5,
        target_y: 2.5,
        ..FloatingTransform::default()
    })
    .unwrap();
    core.cancel_floating();
    assert_eq!(CoreObservation::capture(&mut core).unwrap(), base);

    assert_eq!(
        core.apply_fill_with_cancel(&fill_request(0, 0, [9, 8, 7, 255]), || true),
        Err(CoreError::Cancelled)
    );
    assert_eq!(CoreObservation::capture(&mut core).unwrap(), base);
}

#[test]
fn redo_branch_savepoint_and_failed_target_creation_remain_observable_contracts() {
    let suffix = TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let normal = std::env::temp_dir().join(format!(
        "inkpod-state-machine-normal-{}-{suffix}.inkpod",
        std::process::id()
    ));
    let recovery = std::env::temp_dir().join(format!(
        "inkpod-state-machine-recovery-{}-{suffix}.inkpod",
        std::process::id()
    ));
    let missing = std::env::temp_dir()
        .join(format!(
            "inkpod-missing-parent-{}-{suffix}",
            std::process::id()
        ))
        .join("failed.inkpod");

    let mut core = seeded_core();
    core.save(&normal).unwrap();
    let clean = CoreObservation::capture(&mut core).unwrap();
    core.invert_selection().unwrap();
    let edited = CoreObservation::capture(&mut core).unwrap();
    assert!(edited.common.document.dirty);
    core.autosave(&recovery).unwrap();
    assert_eq!(CoreObservation::capture(&mut core).unwrap(), edited);
    core.export_common_raster(CommonRasterFormat::Png, false)
        .unwrap();
    assert_eq!(CoreObservation::capture(&mut core).unwrap(), edited);
    assert!(core.save(&missing).is_err());
    assert_eq!(CoreObservation::capture(&mut core).unwrap(), edited);

    core.undo().unwrap();
    assert_eq!(
        CoreObservation::capture(&mut core)
            .unwrap()
            .document_semantics(),
        clean.document_semantics()
    );
    core.apply_selection(
        &SelectionShape::Rectangle(RectI32 {
            x: 1,
            y: 1,
            width: 3,
            height: 3,
        }),
        SelectionOperation::New,
    )
    .unwrap();
    assert!(!core.document_info().unwrap().can_redo);
    assert!(core.redo().is_err());
    core.save(&normal).unwrap();
    assert!(!core.document_info().unwrap().dirty);

    let mut control = seeded_core();
    let primary = core.document_info().unwrap().layer_id;
    let failed_before = CoreObservation::capture(&mut core).unwrap();
    assert!(
        core.create_plane(primary, PixelFormat::BinaryMask8, "Invalid")
            .is_err()
    );
    assert_eq!(CoreObservation::capture(&mut core).unwrap(), failed_before);
    let (_, after_failed_id) = core.create_layer("After failed ID").unwrap();
    let (_, control_id) = control.create_layer("Control ID").unwrap();
    assert_eq!(
        after_failed_id, control_id,
        "target-changing topology must stage stable IDs until commit"
    );

    fs::remove_file(normal).unwrap();
    fs::remove_file(recovery).unwrap();
}
