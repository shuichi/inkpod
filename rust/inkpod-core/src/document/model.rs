use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlaneNode {
    pub(crate) id: u64,
    pub(crate) kind: PlaneType,
    pub(crate) name: String,
    pub(crate) visible: bool,
    pub(crate) editable: bool,
    pub(crate) opacity_milli: u32,
    pub(crate) raster: TileRaster,
}

impl PlaneNode {
    pub(crate) fn info(&self) -> PlaneInfo {
        PlaneInfo {
            id: self.id,
            kind: self.kind,
            pixel_format: self.raster.format(),
            name: self.name.clone(),
            visible: self.visible,
            editable: self.editable,
            opacity_milli: self.opacity_milli,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LayerNode {
    pub(crate) id: u64,
    pub(crate) kind: LayerKind,
    pub(crate) name: String,
    pub(crate) visible: bool,
    pub(crate) editable: bool,
    pub(crate) opacity_milli: u32,
    pub(crate) planes: Vec<PlaneNode>,
}

impl LayerNode {
    pub(crate) fn info(&self) -> LayerInfo {
        LayerInfo {
            id: self.id,
            kind: self.kind,
            name: self.name.clone(),
            visible: self.visible,
            editable: self.editable,
            opacity_milli: self.opacity_milli,
            planes: self.planes.iter().map(PlaneNode::info).collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CellDocument {
    pub(crate) uuid: u128,
    pub(crate) id: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) dpi_x_milli: u32,
    pub(crate) dpi_y_milli: u32,
    pub(crate) frames: FrameMetadata,
    pub(crate) main_line_color: PixelValue,
    pub(crate) palette: Palette,
    pub(crate) layers: Vec<LayerNode>,
    pub(crate) active_layer_id: u64,
    pub(crate) active_plane_id: u64,
    pub(crate) selection_plane_id: u64,
    pub(crate) selection: TileRaster,
    pub(crate) guides: Vec<Guide>,
    pub(crate) grid: GridConfig,
    pub(crate) light_table: animation::LightTableState,
    pub(crate) vector: vector::VectorState,
    pub(crate) adjustments: BTreeMap<u64, Adjustment>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DocumentIds {
    pub(crate) document: u64,
    pub(crate) layer: u64,
    pub(crate) main_plane: u64,
    pub(crate) color_plane: u64,
    pub(crate) selection_plane: u64,
    pub(crate) light_table_set: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PaperSpec {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) dpi_x_milli: u32,
    pub(crate) dpi_y_milli: u32,
}

impl CellDocument {
    pub(crate) fn new(ids: DocumentIds, uuid: u128, paper: PaperSpec) -> Result<Self, CoreError> {
        if paper.dpi_x_milli == 0 || paper.dpi_y_milli == 0 {
            return Err(CoreError::InvalidArgument("DPI must be nonzero"));
        }
        if uuid == 0 {
            return Err(CoreError::InvalidArgument("document UUID must be nonzero"));
        }
        let full = RectI32 {
            x: 0,
            y: 0,
            width: paper
                .width
                .try_into()
                .map_err(|_| CoreError::InvalidArgument("width exceeds frame range"))?,
            height: paper
                .height
                .try_into()
                .map_err(|_| CoreError::InvalidArgument("height exceeds frame range"))?,
        };
        let inset_x = (paper.width / 20) as i32;
        let inset_y = (paper.height / 20) as i32;
        let frames = FrameMetadata {
            hundred_frame: full,
            reference_frame: RectI32 {
                x: (paper.width / 2) as i32,
                y: (paper.height / 2) as i32,
                width: full.width,
                height: full.height,
            },
            drawing_frame: full,
            safe_frame: RectI32 {
                x: inset_x,
                y: inset_y,
                width: full.width - inset_x * 2,
                height: full.height - inset_y * 2,
            },
            margins: Margins::default(),
        };
        let main_plane = PlaneNode {
            id: ids.main_plane,
            kind: PlaneType::MainLine,
            name: "Main Line".to_owned(),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            raster: TileRaster::new(paper.width, paper.height, PixelFormat::BinaryMask8)?,
        };
        let color_plane = PlaneNode {
            id: ids.color_plane,
            kind: PlaneType::Color,
            name: "Color".to_owned(),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            raster: TileRaster::new(paper.width, paper.height, PixelFormat::StraightRgba8)?,
        };
        Ok(Self {
            uuid,
            id: ids.document,
            width: paper.width,
            height: paper.height,
            dpi_x_milli: paper.dpi_x_milli,
            dpi_y_milli: paper.dpi_y_milli,
            frames,
            main_line_color: PixelValue::Rgba([0, 0, 0, 255]),
            palette: Palette::default(),
            layers: vec![LayerNode {
                id: ids.layer,
                kind: LayerKind::BinaryColoring,
                name: "Coloring Layer".to_owned(),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                planes: vec![main_plane, color_plane],
            }],
            active_layer_id: ids.layer,
            active_plane_id: ids.main_plane,
            selection_plane_id: ids.selection_plane,
            selection: TileRaster::new(paper.width, paper.height, PixelFormat::BinaryMask8)?,
            guides: Vec::new(),
            grid: GridConfig::default(),
            light_table: animation::LightTableState::new(ids.light_table_set),
            vector: vector::VectorState::default(),
            adjustments: BTreeMap::new(),
        })
    }

    pub(crate) fn to_file(&self) -> CellFile {
        let (layer_id, main_plane_id, color_plane_id) = self.primary_ids();
        let mut planes: Vec<_> = self
            .layers
            .iter()
            .flat_map(|layer| layer.planes.iter())
            .map(|plane| raster_to_file_plane(plane.id, plane.kind.file_kind(), &plane.raster))
            .collect();
        planes.push(raster_to_file_plane(
            self.selection_plane_id,
            FilePlaneKind::Selection,
            &self.selection,
        ));
        planes.extend(self.light_table.file_planes());
        CellFile {
            document_uuid: self.uuid.to_le_bytes(),
            document_id: self.id,
            layer_id,
            main_plane_id,
            color_plane_id,
            width: self.width,
            height: self.height,
            dpi_x_milli: self.dpi_x_milli,
            dpi_y_milli: self.dpi_y_milli,
            frames: self.frames,
            main_line_color: self.main_line_color,
            palette: self.palette.colors().to_vec(),
            planes,
            document_metadata: Some(FileDocumentMetadata {
                active_layer_id: self.active_layer_id,
                active_plane_id: self.active_plane_id,
                selection_plane_id: self.selection_plane_id,
                layers: self
                    .layers
                    .iter()
                    .map(|layer| FileLayer {
                        id: layer.id,
                        kind: layer.kind,
                        name: layer.name.clone(),
                        visible: layer.visible,
                        editable: layer.editable,
                        opacity_milli: layer.opacity_milli,
                        planes: layer
                            .planes
                            .iter()
                            .map(|plane| FilePlaneProperties {
                                id: plane.id,
                                name: plane.name.clone(),
                                visible: plane.visible,
                                editable: plane.editable,
                                opacity_milli: plane.opacity_milli,
                            })
                            .collect(),
                    })
                    .collect(),
                guides: self
                    .guides
                    .iter()
                    .map(|guide| FileGuide {
                        id: guide.id,
                        axis: guide.axis,
                        position: guide.position,
                    })
                    .collect(),
                grid: FileGrid {
                    origin_x: self.grid.origin_x,
                    origin_y: self.grid.origin_y,
                    spacing_x: self.grid.spacing_x,
                    spacing_y: self.grid.spacing_y,
                    subdivisions: self.grid.subdivisions,
                },
            }),
            light_table_metadata: Some(self.light_table.to_file()),
            vector_metadata: self.vector.to_file(
                self.layers
                    .iter()
                    .any(|layer| layer.kind == LayerKind::VectorColoring),
            ),
            adjustment_metadata: (!self.adjustments.is_empty()).then(|| FileAdjustmentMetadata {
                adjustments: self
                    .adjustments
                    .iter()
                    .map(|(layer_id, adjustment)| FileAdjustmentLayer {
                        layer_id: *layer_id,
                        adjustment: adjustment.clone(),
                    })
                    .collect(),
            }),
        }
    }

    pub(crate) fn from_file(file: CellFile, revision: u64) -> Result<Self, CoreError> {
        let main_file = file
            .planes
            .iter()
            .find(|plane| plane.kind == FilePlaneKind::MainLine)
            .ok_or(CoreError::InvalidState("main line plane is missing"))?;
        let color_file = file
            .planes
            .iter()
            .find(|plane| plane.kind == FilePlaneKind::Color)
            .ok_or(CoreError::InvalidState("color plane is missing"))?;
        let mut palette = Palette::default();
        for color in &file.palette {
            palette.push(*color)?;
        }
        let (layers, active_layer_id, active_plane_id, selection_plane_id, selection, guides, grid) =
            if let Some(metadata) = &file.document_metadata {
                let mut layers = Vec::with_capacity(metadata.layers.len());
                for layer in &metadata.layers {
                    let mut planes = Vec::with_capacity(layer.planes.len());
                    for properties in &layer.planes {
                        let payload = file
                            .planes
                            .iter()
                            .find(|plane| plane.id == properties.id)
                            .ok_or(CoreError::InvalidState("layer plane payload is missing"))?;
                        planes.push(PlaneNode {
                            id: properties.id,
                            kind: PlaneType::from_file(payload.kind),
                            name: properties.name.clone(),
                            visible: properties.visible,
                            editable: properties.editable,
                            opacity_milli: properties.opacity_milli,
                            raster: file_plane_to_raster(payload, revision)?,
                        });
                    }
                    validate_layer_kind(layer.kind, &planes)?;
                    layers.push(LayerNode {
                        id: layer.id,
                        kind: layer.kind,
                        name: layer.name.clone(),
                        visible: layer.visible,
                        editable: layer.editable,
                        opacity_milli: layer.opacity_milli,
                        planes,
                    });
                }
                let selection_file = file
                    .planes
                    .iter()
                    .find(|plane| plane.id == metadata.selection_plane_id)
                    .ok_or(CoreError::InvalidState("selection payload is missing"))?;
                (
                    layers,
                    metadata.active_layer_id,
                    metadata.active_plane_id,
                    metadata.selection_plane_id,
                    file_plane_to_raster(selection_file, revision)?,
                    metadata
                        .guides
                        .iter()
                        .map(|guide| Guide {
                            id: guide.id,
                            axis: guide.axis,
                            position: guide.position,
                        })
                        .collect(),
                    GridConfig {
                        origin_x: metadata.grid.origin_x,
                        origin_y: metadata.grid.origin_y,
                        spacing_x: metadata.grid.spacing_x,
                        spacing_y: metadata.grid.spacing_y,
                        subdivisions: metadata.grid.subdivisions,
                    },
                )
            } else {
                let selection_plane_id = file
                    .planes
                    .iter()
                    .map(|plane| plane.id)
                    .chain([file.document_id, file.layer_id])
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or(CoreError::InvalidState("selection ID overflow"))?;
                let layer_kind = if matches!(
                    main_file.pixel_format,
                    PixelFormat::Grayscale8 | PixelFormat::Grayscale16
                ) {
                    LayerKind::GrayscaleColoring
                } else {
                    LayerKind::BinaryColoring
                };
                (
                    vec![LayerNode {
                        id: file.layer_id,
                        kind: layer_kind,
                        name: "Coloring Layer".to_owned(),
                        visible: true,
                        editable: true,
                        opacity_milli: 1_000,
                        planes: vec![
                            PlaneNode {
                                id: file.main_plane_id,
                                kind: PlaneType::MainLine,
                                name: "Main Line".to_owned(),
                                visible: true,
                                editable: true,
                                opacity_milli: 1_000,
                                raster: file_plane_to_raster(main_file, revision)?,
                            },
                            PlaneNode {
                                id: file.color_plane_id,
                                kind: PlaneType::Color,
                                name: "Color".to_owned(),
                                visible: true,
                                editable: true,
                                opacity_milli: 1_000,
                                raster: file_plane_to_raster(color_file, revision)?,
                            },
                        ],
                    }],
                    file.layer_id,
                    file.main_plane_id,
                    selection_plane_id,
                    TileRaster::new(file.width, file.height, PixelFormat::BinaryMask8)?,
                    Vec::new(),
                    GridConfig::default(),
                )
            };
        let legacy_light_table_set_id = file
            .planes
            .iter()
            .map(|plane| plane.id)
            .chain(file.document_metadata.iter().flat_map(|metadata| {
                metadata
                    .layers
                    .iter()
                    .map(|layer| layer.id)
                    .chain(metadata.guides.iter().map(|guide| guide.id))
            }))
            .chain([file.document_id, selection_plane_id])
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(CoreError::InvalidState("light-table set ID overflow"))?;
        let light_table = animation::LightTableState::from_file(
            file.light_table_metadata.as_ref(),
            &file.planes,
            revision,
            legacy_light_table_set_id,
        )?;
        let vector = vector::VectorState::from_file(file.vector_metadata.as_ref());
        let adjustments = file
            .adjustment_metadata
            .as_ref()
            .map(|metadata| {
                metadata
                    .adjustments
                    .iter()
                    .map(|layer| (layer.layer_id, layer.adjustment.clone()))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            uuid: u128::from_le_bytes(file.document_uuid),
            id: file.document_id,
            width: file.width,
            height: file.height,
            dpi_x_milli: file.dpi_x_milli,
            dpi_y_milli: file.dpi_y_milli,
            frames: file.frames,
            main_line_color: file.main_line_color,
            palette,
            layers,
            active_layer_id,
            active_plane_id,
            selection_plane_id,
            selection,
            guides,
            grid,
            light_table,
            vector,
            adjustments,
        })
    }

    pub(crate) fn primary_layer(&self) -> &LayerNode {
        self.layers
            .iter()
            .find(|layer| {
                layer
                    .planes
                    .iter()
                    .any(|plane| plane.kind == PlaneType::MainLine)
                    && layer
                        .planes
                        .iter()
                        .any(|plane| plane.kind == PlaneType::Color)
            })
            .expect("validated coloring document must retain a coloring layer")
    }

    pub(crate) fn primary_ids(&self) -> (u64, u64, u64) {
        let layer = self.primary_layer();
        let main = layer
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneType::MainLine)
            .expect("validated coloring layer has main plane");
        let color = layer
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneType::Color)
            .expect("validated coloring layer has color plane");
        (layer.id, main.id, color.id)
    }

    pub(crate) fn plane_for_role(&self, role: ActivePlane) -> Result<&PlaneNode, CoreError> {
        let kind = match role {
            ActivePlane::MainLine => PlaneType::MainLine,
            ActivePlane::Color => PlaneType::Color,
        };
        self.layers
            .iter()
            .find(|layer| layer.id == self.active_layer_id)
            .and_then(|layer| layer.planes.iter().find(|plane| plane.kind == kind))
            .or_else(|| {
                self.layers
                    .iter()
                    .flat_map(|layer| layer.planes.iter())
                    .find(|plane| plane.kind == kind)
            })
            .ok_or(CoreError::InvalidState(
                "requested plane role is unavailable",
            ))
    }

    pub(crate) fn plane_for_role_mut(
        &mut self,
        role: ActivePlane,
    ) -> Result<&mut PlaneNode, CoreError> {
        let kind = match role {
            ActivePlane::MainLine => PlaneType::MainLine,
            ActivePlane::Color => PlaneType::Color,
        };
        let preferred = self.active_layer_id;
        let preferred_index = self.layers.iter().position(|layer| layer.id == preferred);
        if let Some(index) = preferred_index
            && let Some(plane_index) = self.layers[index]
                .planes
                .iter()
                .position(|plane| plane.kind == kind)
        {
            return Ok(&mut self.layers[index].planes[plane_index]);
        }
        for layer in &mut self.layers {
            if let Some(index) = layer.planes.iter().position(|plane| plane.kind == kind) {
                return Ok(&mut layer.planes[index]);
            }
        }
        Err(CoreError::InvalidState(
            "requested plane role is unavailable",
        ))
    }

    pub(crate) fn raster(&self, plane: ActivePlane) -> &TileRaster {
        &self
            .plane_for_role(plane)
            .expect("validated coloring document must retain required planes")
            .raster
    }

    pub(crate) fn raster_mut(&mut self, plane: ActivePlane) -> &mut TileRaster {
        &mut self
            .plane_for_role_mut(plane)
            .expect("validated coloring document must retain required planes")
            .raster
    }

    pub(crate) fn active_plane_role(&self) -> ActivePlane {
        self.layers
            .iter()
            .flat_map(|layer| layer.planes.iter())
            .find(|plane| plane.id == self.active_plane_id)
            .map_or(ActivePlane::Color, |plane| match plane.kind {
                PlaneType::MainLine => ActivePlane::MainLine,
                _ => ActivePlane::Color,
            })
    }

    pub(crate) fn plane_by_id(&self, id: u64) -> Option<&PlaneNode> {
        self.layers
            .iter()
            .flat_map(|layer| layer.planes.iter())
            .find(|plane| plane.id == id)
    }

    pub(crate) fn plane_by_id_mut(&mut self, id: u64) -> Option<&mut PlaneNode> {
        self.layers
            .iter_mut()
            .flat_map(|layer| layer.planes.iter_mut())
            .find(|plane| plane.id == id)
    }

    pub(crate) fn max_stable_id(&self) -> u64 {
        self.layers
            .iter()
            .flat_map(|layer| {
                std::iter::once(layer.id).chain(layer.planes.iter().map(|plane| plane.id))
            })
            .chain(self.guides.iter().map(|guide| guide.id))
            .chain([self.light_table.maximum_id()])
            .chain([self.vector.maximum_id()])
            .chain([self.id, self.selection_plane_id])
            .max()
            .unwrap_or(0)
    }
}
