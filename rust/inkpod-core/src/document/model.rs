use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlaneNode {
    pub(crate) id: PlaneId,
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
            id: self.id.get(),
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
    pub(crate) id: LayerId,
    pub(crate) name: String,
    pub(crate) visible: bool,
    pub(crate) editable: bool,
    pub(crate) opacity_milli: u32,
    pub(crate) planes: Vec<PlaneNode>,
}

impl LayerNode {
    pub(crate) fn info(&self) -> LayerInfo {
        LayerInfo {
            id: self.id.get(),
            name: self.name.clone(),
            visible: self.visible,
            editable: self.editable,
            opacity_milli: self.opacity_milli,
            planes: self.planes.iter().map(PlaneNode::info).collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SavedSelectionMask {
    pub(crate) id: SavedSelectionId,
    pub(crate) name: String,
    pub(crate) raster: TileRaster,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CellDocument {
    pub(crate) uuid: u128,
    pub(crate) id: DocumentId,
    pub(crate) cell_id: CellId,
    pub(crate) base_surface: BaseSurface,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) dpi_x_milli: u32,
    pub(crate) dpi_y_milli: u32,
    pub(crate) frames: FrameMetadata,
    pub(crate) main_line_color: PixelValue,
    pub(crate) palette: Palette,
    pub(crate) color_chart: ColorChart,
    pub(crate) layers: Vec<LayerNode>,
    pub(crate) selection_plane_id: PlaneId,
    pub(crate) selection: TileRaster,
    pub(crate) saved_selection_masks: Vec<SavedSelectionMask>,
    pub(crate) fill_protection_plane_id: PlaneId,
    pub(crate) fill_protection: TileRaster,
    pub(crate) guides: Vec<Guide>,
    pub(crate) grid: GridConfig,
    pub(crate) light_table: animation::LightTableState,
    pub(crate) shooting_frame: Option<ShootingFrameObject>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DocumentIds {
    pub(crate) document: DocumentId,
    pub(crate) layer: LayerId,
    pub(crate) main_plane: PlaneId,
    pub(crate) color_plane: PlaneId,
    pub(crate) selection_plane: PlaneId,
    pub(crate) fill_protection_plane: PlaneId,
    pub(crate) light_table_set: LightTableSetId,
    pub(crate) cell: CellId,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PaperSpec {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) dpi_x_milli: u32,
    pub(crate) dpi_y_milli: u32,
}

impl CellDocument {
    pub(crate) fn logical_raster_usage(&self) -> (u64, u64) {
        let mut tile_count = self.selection.allocated_tile_count() as u64
            + self.fill_protection.allocated_tile_count() as u64;
        let mut tile_bytes = self
            .selection
            .allocated_tile_bytes()
            .saturating_add(self.fill_protection.allocated_tile_bytes());
        for plane in self.layers.iter().flat_map(|layer| &layer.planes) {
            tile_count = tile_count.saturating_add(plane.raster.allocated_tile_count() as u64);
            tile_bytes = tile_bytes.saturating_add(plane.raster.allocated_tile_bytes());
        }
        for saved_selection in &self.saved_selection_masks {
            tile_count =
                tile_count.saturating_add(saved_selection.raster.allocated_tile_count() as u64);
            tile_bytes = tile_bytes.saturating_add(saved_selection.raster.allocated_tile_bytes());
        }
        (tile_count, tile_bytes)
    }

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
            shooting_frame: full,
            maximum_close_frame: full,
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
            cell_id: ids.cell,
            base_surface: BaseSurface::SolidWhite,
            width: paper.width,
            height: paper.height,
            dpi_x_milli: paper.dpi_x_milli,
            dpi_y_milli: paper.dpi_y_milli,
            frames,
            main_line_color: PixelValue::Rgba([0, 0, 0, 255]),
            palette: Palette::default(),
            color_chart: ColorChart::default(),
            layers: vec![LayerNode {
                id: ids.layer,
                name: "Coloring Layer".to_owned(),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                planes: vec![main_plane, color_plane],
            }],
            selection_plane_id: ids.selection_plane,
            selection: TileRaster::new(paper.width, paper.height, PixelFormat::BinaryMask8)?,
            saved_selection_masks: Vec::new(),
            fill_protection_plane_id: ids.fill_protection_plane,
            fill_protection: TileRaster::new(paper.width, paper.height, PixelFormat::BinaryMask8)?,
            guides: Vec::new(),
            grid: GridConfig::default(),
            light_table: animation::LightTableState::new(ids.light_table_set),
            shooting_frame: None,
        })
    }

    pub(crate) fn to_archive(&self) -> DocumentArchive {
        let (layer_id, main_plane_id, color_plane_id) = self.primary_ids();
        let mut planes: Vec<_> = self
            .layers
            .iter()
            .flat_map(|layer| layer.planes.iter())
            .map(|plane| {
                raster_to_file_plane(plane.id.get(), plane.kind.file_kind(), &plane.raster)
            })
            .collect();
        planes.push(raster_to_file_plane(
            self.selection_plane_id.get(),
            FilePlaneKind::CurrentSelection,
            &self.selection,
        ));
        planes.extend(self.saved_selection_masks.iter().map(|saved_selection| {
            raster_to_file_plane(
                saved_selection.id.get(),
                FilePlaneKind::SavedSelection,
                &saved_selection.raster,
            )
        }));
        planes.push(raster_to_file_plane(
            self.fill_protection_plane_id.get(),
            FilePlaneKind::FillProtection,
            &self.fill_protection,
        ));
        planes.extend(self.light_table.file_planes());
        DocumentArchive {
            document_uuid: self.uuid.to_le_bytes(),
            document_id: self.id.get(),
            cell_id: self.cell_id.get(),
            layer_id: layer_id.get(),
            main_plane_id: main_plane_id.get(),
            color_plane_id: color_plane_id.get(),
            width: self.width,
            height: self.height,
            dpi_x_milli: self.dpi_x_milli,
            dpi_y_milli: self.dpi_y_milli,
            frames: self.frames,
            main_line_color: self.main_line_color,
            palette: self.palette.colors().to_vec(),
            planes,
            document_metadata: Some(FileDocumentMetadata {
                // The Genesis archive retains these historical DTO fields for
                // canonical byte stability. The authoritative EditorState is
                // stored separately in the native EDIT section.
                active_layer_id: layer_id.get(),
                active_plane_id: main_plane_id.get(),
                selection_plane_id: self.selection_plane_id.get(),
                fill_protection_plane_id: self.fill_protection_plane_id.get(),
                layers: self
                    .layers
                    .iter()
                    .map(|layer| FileLayer {
                        id: layer.id.get(),
                        name: layer.name.clone(),
                        visible: layer.visible,
                        editable: layer.editable,
                        opacity_milli: layer.opacity_milli,
                        planes: layer
                            .planes
                            .iter()
                            .map(|plane| FilePlaneProperties {
                                id: plane.id.get(),
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
                color_chart: FileColorChart {
                    entries: self
                        .color_chart
                        .entries()
                        .iter()
                        .map(|entry| FileColorChartEntry {
                            color: application_color(entry.color),
                            name: entry.name.clone(),
                        })
                        .collect(),
                },
                color_chart_locked: self.color_chart.locked(),
                shooting_frame: self.shooting_frame.map(|frame| FileShootingFrame {
                    id: frame.id.get(),
                    center_x_milli: frame.input.center_x_milli,
                    center_y_milli: frame.input.center_y_milli,
                    width_milli: frame.input.width_milli,
                    height_milli: frame.input.height_milli,
                    rotation_turns: frame.input.rotation_turns,
                    anchor: match frame.input.anchor {
                        ShootingFrameAnchor::TopLeft => FileShootingFrameAnchor::TopLeft,
                        ShootingFrameAnchor::TopRight => FileShootingFrameAnchor::TopRight,
                        ShootingFrameAnchor::Center => FileShootingFrameAnchor::Center,
                        ShootingFrameAnchor::BottomLeft => FileShootingFrameAnchor::BottomLeft,
                        ShootingFrameAnchor::BottomRight => FileShootingFrameAnchor::BottomRight,
                    },
                    visible: frame.input.visible,
                }),
                saved_selections: self
                    .saved_selection_masks
                    .iter()
                    .map(|saved_selection| inkpod_format::FileSavedSelection {
                        id: saved_selection.id.get(),
                        name: saved_selection.name.clone(),
                    })
                    .collect(),
            }),
            light_table_metadata: Some(self.light_table.to_file()),
        }
    }

    pub(crate) fn from_archive(
        file: DocumentArchive,
        revision: DocumentRevision,
    ) -> Result<Self, CoreError> {
        let metadata = file
            .document_metadata
            .as_ref()
            .ok_or(CoreError::InvalidState(
                "current document metadata is missing",
            ))?;
        let mut palette = Palette::default();
        for color in &file.palette {
            palette.push(*color)?;
        }
        let color_chart = ColorChart::validated(
            metadata
                .color_chart
                .entries
                .iter()
                .map(|entry| ColorChartEntry {
                    color: pixel_value(entry.color),
                    name: entry.name.clone(),
                })
                .collect(),
            metadata.color_chart_locked,
        )?;
        let (
            layers,
            selection_plane_id,
            selection,
            saved_selection_masks,
            fill_protection_plane_id,
            fill_protection,
            guides,
            grid,
        ) = {
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
                        id: PlaneId::from_raw(properties.id),
                        kind: PlaneType::from_file(payload.kind)
                            .ok_or(CoreError::InvalidState("layer contains a non-image plane"))?,
                        name: properties.name.clone(),
                        visible: properties.visible,
                        editable: properties.editable,
                        opacity_milli: properties.opacity_milli,
                        raster: file_plane_to_raster(payload, revision.get())?,
                    });
                }
                validate_layer(&planes)?;
                layers.push(LayerNode {
                    id: LayerId::from_raw(layer.id),
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
            let fill_protection_file = file
                .planes
                .iter()
                .find(|plane| plane.id == metadata.fill_protection_plane_id)
                .ok_or(CoreError::InvalidState(
                    "fill-protection payload is missing",
                ))?;
            let saved_selection_masks = metadata
                .saved_selections
                .iter()
                .map(|saved_selection| {
                    let payload = file
                        .planes
                        .iter()
                        .find(|plane| plane.id == saved_selection.id)
                        .ok_or(CoreError::InvalidState(
                            "saved-selection payload is missing",
                        ))?;
                    Ok(SavedSelectionMask {
                        id: SavedSelectionId::from_raw(saved_selection.id)
                            .ok_or(CoreError::InvalidState("saved-selection ID is invalid"))?,
                        name: saved_selection.name.clone(),
                        raster: file_plane_to_raster(payload, revision.get())?,
                    })
                })
                .collect::<Result<Vec<_>, CoreError>>()?;
            (
                layers,
                PlaneId::from_raw(metadata.selection_plane_id),
                file_plane_to_raster(selection_file, revision.get())?,
                saved_selection_masks,
                PlaneId::from_raw(metadata.fill_protection_plane_id),
                file_plane_to_raster(fill_protection_file, revision.get())?,
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
                    .chain(
                        metadata
                            .saved_selections
                            .iter()
                            .map(|selection| selection.id),
                    )
                    .chain(metadata.shooting_frame.iter().map(|frame| frame.id))
            }))
            .chain([
                file.document_id,
                file.cell_id,
                selection_plane_id.get(),
                fill_protection_plane_id.get(),
            ])
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(CoreError::InvalidState("light-table set ID overflow"))?;
        let light_table = animation::LightTableState::from_file(
            file.light_table_metadata.as_ref(),
            &file.planes,
            revision,
            LightTableSetId::from_raw(legacy_light_table_set_id),
        )?;
        let shooting_frame = file.document_metadata.as_ref().and_then(|metadata| {
            metadata.shooting_frame.map(|frame| ShootingFrameObject {
                id: ShootingFrameId::from_raw(frame.id),
                input: ShootingFrameInput {
                    center_x_milli: frame.center_x_milli,
                    center_y_milli: frame.center_y_milli,
                    width_milli: frame.width_milli,
                    height_milli: frame.height_milli,
                    rotation_turns: frame.rotation_turns,
                    anchor: match frame.anchor {
                        FileShootingFrameAnchor::TopLeft => ShootingFrameAnchor::TopLeft,
                        FileShootingFrameAnchor::TopRight => ShootingFrameAnchor::TopRight,
                        FileShootingFrameAnchor::Center => ShootingFrameAnchor::Center,
                        FileShootingFrameAnchor::BottomLeft => ShootingFrameAnchor::BottomLeft,
                        FileShootingFrameAnchor::BottomRight => ShootingFrameAnchor::BottomRight,
                    },
                    visible: frame.visible,
                },
            })
        });
        let document = Self {
            uuid: u128::from_le_bytes(file.document_uuid),
            id: DocumentId::from_raw(file.document_id),
            cell_id: CellId::from_raw(file.cell_id),
            // The caller installs the validated GENS base discriminant after decoding.
            base_surface: BaseSurface::SolidWhite,
            width: file.width,
            height: file.height,
            dpi_x_milli: file.dpi_x_milli,
            dpi_y_milli: file.dpi_y_milli,
            frames: file.frames,
            main_line_color: file.main_line_color,
            palette,
            color_chart,
            layers,
            selection_plane_id,
            selection,
            saved_selection_masks,
            fill_protection_plane_id,
            fill_protection,
            guides,
            grid,
            light_table,
            shooting_frame,
        };
        if let Some(frame) = document.shooting_frame {
            crate::shooting_frame::validate_shooting_frame_input(frame.input)?;
        }
        Ok(document)
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

    pub(crate) fn primary_ids(&self) -> (LayerId, PlaneId, PlaneId) {
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
            .flat_map(|layer| layer.planes.iter())
            .find(|plane| plane.kind == kind)
            .ok_or(CoreError::InvalidState(
                "requested plane role is unavailable",
            ))
    }

    pub(crate) fn plane_for_paint_role(
        &self,
        role: ActivePlane,
        active_layer_id: Option<LayerId>,
        active_plane_id: Option<PlaneId>,
    ) -> Result<&PlaneNode, CoreError> {
        if role == ActivePlane::Color
            && let Some(active) = active_plane_id.and_then(|id| self.plane_by_id(id))
            && active.kind == PlaneType::Raster
        {
            return Ok(active);
        }
        let kind = match role {
            ActivePlane::MainLine => PlaneType::MainLine,
            ActivePlane::Color => PlaneType::Color,
        };
        active_layer_id
            .and_then(|preferred| self.layers.iter().find(|layer| layer.id == preferred))
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

    #[cfg(test)]
    pub(crate) fn raster_mut(&mut self, plane: ActivePlane) -> &mut TileRaster {
        &mut self
            .plane_for_role_mut(plane)
            .expect("validated coloring document must retain required planes")
            .raster
    }

    pub(crate) fn active_plane_role(&self, active_plane_id: Option<PlaneId>) -> ActivePlane {
        active_plane_id
            .and_then(|id| self.plane_by_id(id))
            .map_or(ActivePlane::Color, |plane| match plane.kind {
                PlaneType::MainLine => ActivePlane::MainLine,
                _ => ActivePlane::Color,
            })
    }

    pub(crate) fn plane_by_id(&self, id: PlaneId) -> Option<&PlaneNode> {
        self.layers
            .iter()
            .flat_map(|layer| layer.planes.iter())
            .find(|plane| plane.id == id)
    }

    pub(crate) fn plane_by_id_mut(&mut self, id: PlaneId) -> Option<&mut PlaneNode> {
        self.layers
            .iter_mut()
            .flat_map(|layer| layer.planes.iter_mut())
            .find(|plane| plane.id == id)
    }

    pub(crate) fn max_stable_id(&self) -> u64 {
        self.layers
            .iter()
            .flat_map(|layer| {
                std::iter::once(layer.id.get())
                    .chain(layer.planes.iter().map(|plane| plane.id.get()))
            })
            .chain(self.guides.iter().map(|guide| guide.id))
            .chain([self.light_table.maximum_id()])
            .chain(self.shooting_frame.iter().map(|object| object.id.get()))
            .chain(
                self.saved_selection_masks
                    .iter()
                    .map(|selection| selection.id.get()),
            )
            .chain([
                self.id.get(),
                self.cell_id.get(),
                self.selection_plane_id.get(),
                self.fill_protection_plane_id.get(),
            ])
            .max()
            .unwrap_or(0)
    }
}

fn application_color(color: PixelValue) -> ApplicationColor {
    match color {
        PixelValue::Rgba(channels) => ApplicationColor {
            depth: 8,
            red: u16::from(channels[0]),
            green: u16::from(channels[1]),
            blue: u16::from(channels[2]),
            alpha: u16::from(channels[3]),
        },
        PixelValue::Rgba16(channels) => ApplicationColor {
            depth: 16,
            red: channels[0],
            green: channels[1],
            blue: channels[2],
            alpha: channels[3],
        },
        PixelValue::Binary(_) | PixelValue::Grayscale8(_) | PixelValue::Grayscale16(_) => {
            unreachable!("validated Color chart entries are RGBA")
        }
    }
}

fn pixel_value(color: ApplicationColor) -> PixelValue {
    let channels = [color.red, color.green, color.blue, color.alpha];
    if color.depth == 8 {
        PixelValue::Rgba(channels.map(|channel| channel as u8))
    } else {
        PixelValue::Rgba16(channels)
    }
}
