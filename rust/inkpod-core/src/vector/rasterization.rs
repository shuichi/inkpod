use super::geometry::*;
use super::model::*;
use super::*;
use crate::EditorTarget;
use crate::primitive::CanonicalInvocation;

impl Core {
    /// Rasterizes one vector-coloring layer into owned straight-alpha RGBA8 pixels.
    ///
    /// `scale` is in `1..=16`; the operation is read-only and bounded by the vector
    /// raster pixel limit.
    pub fn rasterize_vector_layer(
        &self,
        layer_id: u64,
        scale: u32,
        antialias: bool,
    ) -> Result<VectorRaster, CoreError> {
        let (width, height, stride_bytes, _) = self.vector_raster_layout(layer_id, scale)?;
        self.rasterize_vector_layer_dimensions(
            LayerId::from_raw(layer_id),
            width,
            height,
            stride_bytes,
            antialias,
        )
    }

    pub(super) fn rasterize_vector_layer_dimensions(
        &self,
        layer_id: LayerId,
        width: u32,
        height: u32,
        stride_bytes: u32,
        antialias: bool,
    ) -> Result<VectorRaster, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let layer = document
            .layers
            .iter()
            .find(|layer| layer.id == layer_id && layer.kind == LayerKind::VectorColoring)
            .ok_or(CoreError::InvalidArgument("vector layer ID does not exist"))?;
        rasterize_vector_layer_content(document, layer, width, height, stride_bytes, antialias)
    }
    /// Rasterizes a vector-coloring layer into a new RGBA8 raster layer as one
    /// document transaction. The source vector geometry remains unchanged.
    pub fn rasterize_vector_layer_to_document(
        &mut self,
        layer_id: u64,
        antialias: bool,
        name: &str,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        if !self.canonical_invocation_is_active() {
            let result =
                self.execute_canonical_invocation(CanonicalInvocation::RasterizeVectorLayer {
                    layer_id,
                    antialias,
                    name: name.to_owned(),
                })?;
            let id = *result.output_ids.first().ok_or(CoreError::InvalidState(
                "rasterize-vector primitive did not return its output ID",
            ))?;
            return Ok((result.dispatch, id));
        }
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        let rasterized = self.rasterize_vector_layer(layer_id, 1, antialias)?;
        let base_revision = self.document_revision;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let mut raster = TileRaster::new(
            rasterized.width,
            rasterized.height,
            PixelFormat::StraightRgba8,
        )?;
        for y in 0..rasterized.height {
            for x in 0..rasterized.width {
                let offset = y as usize * rasterized.stride_bytes as usize + x as usize * 4;
                let color = [
                    rasterized.pixels[offset],
                    rasterized.pixels[offset + 1],
                    rasterized.pixels[offset + 2],
                    rasterized.pixels[offset + 3],
                ];
                if color[3] != 0 {
                    raster.set_pixel(x, y, PixelValue::Rgba(color), revision.get())?;
                }
            }
        }
        let mut next_id = self.next_id;
        let new_layer_id = next_id.take_layer();
        let new_plane_id = next_id.take_plane();
        let mut after = before.clone();
        after.layers.push(LayerNode {
            id: new_layer_id,
            kind: LayerKind::Raster,
            name: unique_layer_name(&after.layers, name),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            planes: vec![PlaneNode {
                id: new_plane_id,
                kind: PlaneType::Raster,
                name: "Rasterized".to_owned(),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                raster,
            }],
        });
        let outcome = self.commit_deferred_document_edit_with_target(
            before,
            after,
            base_revision,
            revision,
            EditorTarget {
                layer_id: new_layer_id.get(),
                plane_id: new_plane_id.get(),
            },
        )?;
        self.next_id = next_id;
        Ok((outcome, new_layer_id.get()))
    }

    /// Computes width, height, RGBA8 stride, and byte count for vector rasterization.
    ///
    /// `scale` is in `1..=16`; all arithmetic and allocation bounds are checked.
    pub fn vector_raster_layout(
        &self,
        layer_id: u64,
        scale: u32,
    ) -> Result<(u32, u32, u32, u64), CoreError> {
        let layer_id = LayerId::from_raw(layer_id);
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        document
            .layers
            .iter()
            .find(|layer| layer.id == layer_id && layer.kind == LayerKind::VectorColoring)
            .ok_or(CoreError::InvalidArgument("vector layer ID does not exist"))?;
        if !(1..=16).contains(&scale) {
            return Err(CoreError::InvalidArgument(
                "vector raster scale is outside bounds",
            ));
        }
        let width = document
            .width
            .checked_mul(scale)
            .ok_or(CoreError::InvalidArgument("vector raster width overflows"))?;
        let height = document
            .height
            .checked_mul(scale)
            .ok_or(CoreError::InvalidArgument("vector raster height overflows"))?;
        let pixel_count = u64::from(width) * u64::from(height);
        if pixel_count > MAX_VECTOR_RASTER_PIXELS {
            return Err(CoreError::InvalidArgument(
                "vector raster exceeds its pixel bound",
            ));
        }
        let stride_bytes = width
            .checked_mul(4)
            .ok_or(CoreError::InvalidArgument("vector raster stride overflows"))?;
        Ok((width, height, stride_bytes, pixel_count * 4))
    }
}

struct SampledFill {
    color: [u8; 4],
    bounds: (f64, f64, f64, f64),
    boundaries: Vec<Vec<FlatSample>>,
}

struct SampledPath {
    color: [u8; 4],
    bounds: (f64, f64, f64, f64),
    samples: Vec<FlatSample>,
}

struct SampledPlane<'a> {
    raster: Option<&'a PlaneNode>,
    fills: Vec<SampledFill>,
    paths: Vec<SampledPath>,
}

pub(crate) fn rasterize_vector_layer_content(
    document: &CellDocument,
    layer: &LayerNode,
    width: u32,
    height: u32,
    stride_bytes: u32,
    antialias: bool,
) -> Result<VectorRaster, CoreError> {
    if width == 0 || height == 0 || stride_bytes != width.saturating_mul(4) {
        return Err(CoreError::InvalidArgument(
            "vector raster dimensions are invalid",
        ));
    }
    let mut sampled_planes = Vec::new();
    for plane in layer.planes.iter().rev().filter(|plane| plane.visible) {
        let fills = document
            .vector
            .fills
            .iter()
            .filter(|fill| fill.plane_id == plane.id)
            .filter_map(|fill| {
                let boundaries = fill
                    .boundary_path_ids
                    .iter()
                    .filter_map(|path_id| {
                        document
                            .vector
                            .paths
                            .iter()
                            .find(|path| path.id == *path_id)
                            .map(|path| flatten_path(path, RASTER_STEPS))
                    })
                    .collect::<Vec<_>>();
                let bounds = sampled_bounds(boundaries.iter().flatten().copied(), 0.0)?;
                Some(SampledFill {
                    color: display_color(fill.color, 1_000, plane.opacity_milli),
                    bounds,
                    boundaries,
                })
            })
            .collect();
        let paths = document
            .vector
            .paths
            .iter()
            .filter(|path| path.plane_id == plane.id)
            .filter_map(|path| {
                let samples = flatten_path(path, RASTER_STEPS);
                let padding = samples
                    .iter()
                    .map(|sample| sample.width * 0.5)
                    .fold(0.0_f64, f64::max);
                sampled_bounds(samples.iter().copied(), padding).map(|bounds| SampledPath {
                    color: display_color(path.color, 1_000, plane.opacity_milli),
                    bounds,
                    samples,
                })
            })
            .collect();
        sampled_planes.push(SampledPlane {
            raster: (plane.kind == PlaneType::Raster).then_some(plane),
            fills,
            paths,
        });
    }
    let offsets: &[(f64, f64)] = if antialias {
        &[
            (0.125, 0.125),
            (0.375, 0.125),
            (0.625, 0.125),
            (0.875, 0.125),
            (0.125, 0.375),
            (0.375, 0.375),
            (0.625, 0.375),
            (0.875, 0.375),
            (0.125, 0.625),
            (0.375, 0.625),
            (0.625, 0.625),
            (0.875, 0.625),
            (0.125, 0.875),
            (0.375, 0.875),
            (0.625, 0.875),
            (0.875, 0.875),
        ]
    } else {
        &[(0.5, 0.5)]
    };
    let mut pixels = vec![0_u8; stride_bytes as usize * height as usize];
    for y in 0..height {
        for x in 0..width {
            let mut accumulated_premultiplied = [0_u64; 3];
            let mut accumulated_alpha = 0_u64;
            for offset in offsets {
                let sample = (
                    (f64::from(x) + offset.0) * f64::from(document.width) / f64::from(width),
                    (f64::from(y) + offset.1) * f64::from(document.height) / f64::from(height),
                );
                let mut value = [0_u8; 4];
                for plane in &sampled_planes {
                    if let Some(raster) = plane.raster {
                        let source_x = (sample.0.floor() as u32).min(document.width - 1);
                        let source_y = (sample.1.floor() as u32).min(document.height - 1);
                        let mut rgba = rgba8(raster.raster.pixel(source_x, source_y)?);
                        rgba[3] = ((u32::from(rgba[3]) * raster.opacity_milli + 500) / 1_000) as u8;
                        value = source_over_rgba(value, rgba);
                    }
                    for fill in &plane.fills {
                        if point_in_rect(sample, fill.bounds)
                            && point_in_sampled_fill(&fill.boundaries, sample)
                        {
                            value = source_over_rgba(value, fill.color);
                        }
                    }
                    for path in &plane.paths {
                        if point_in_rect(sample, path.bounds)
                            && point_on_sampled_stroke(&path.samples, sample)
                        {
                            value = source_over_rgba(value, path.color);
                        }
                    }
                }
                value[3] = ((u32::from(value[3]) * layer.opacity_milli + 500) / 1_000) as u8;
                accumulated_alpha += u64::from(value[3]);
                for channel in 0..3 {
                    accumulated_premultiplied[channel] +=
                        u64::from(value[channel]) * u64::from(value[3]);
                }
            }
            let pixel_offset = y as usize * stride_bytes as usize + x as usize * 4;
            for channel in 0..3 {
                pixels[pixel_offset + channel] = (accumulated_premultiplied[channel]
                    + accumulated_alpha / 2)
                    .checked_div(accumulated_alpha)
                    .unwrap_or(0) as u8;
            }
            pixels[pixel_offset + 3] =
                ((accumulated_alpha + offsets.len() as u64 / 2) / offsets.len() as u64) as u8;
        }
    }
    Ok(VectorRaster {
        width,
        height,
        stride_bytes,
        pixels,
    })
}
