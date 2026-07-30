use super::geometry::*;
use super::model::*;
use super::*;

impl Core {
    /// Selects vector path ranges and fills intersecting a half-open document rectangle.
    ///
    /// The result is owned and deterministically ordered; the query does not mutate
    /// document selection, vector state, revision, or history.
    pub fn vector_select(
        &self,
        bounds: RectI32,
        mode: VectorSelectionMode,
    ) -> Result<VectorSelectionResult, CoreError> {
        if bounds.width <= 0 || bounds.height <= 0 {
            return Err(CoreError::InvalidArgument(
                "vector selection bounds are empty",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let right = bounds
            .x
            .checked_add(bounds.width)
            .ok_or(CoreError::InvalidArgument(
                "vector selection bounds overflow",
            ))?;
        let bottom = bounds
            .y
            .checked_add(bounds.height)
            .ok_or(CoreError::InvalidArgument(
                "vector selection bounds overflow",
            ))?;
        let rect = (
            f64::from(bounds.x),
            f64::from(bounds.y),
            f64::from(right),
            f64::from(bottom),
        );
        let mut result = VectorSelectionResult::default();
        if mode == VectorSelectionMode::Fill {
            let center = ((rect.0 + rect.2) * 0.5, (rect.1 + rect.3) * 0.5);
            for fill in &document.vector.fills {
                if point_in_fill(&document.vector, fill, center) {
                    result.fill_ids.push(fill.id.get());
                }
            }
            return Ok(result);
        }
        if mode == VectorSelectionMode::FillBoundary {
            let center = ((rect.0 + rect.2) * 0.5, (rect.1 + rect.3) * 0.5);
            let mut selected = BTreeSet::new();
            for fill in &document.vector.fills {
                if point_in_fill(&document.vector, fill, center) {
                    selected.extend(fill.boundary_path_ids.iter().copied());
                }
            }
            for path_id in selected {
                result.path_ranges.push(full_selection(path_id));
            }
            return Ok(result);
        }
        for path in &document.vector.paths {
            let samples = flatten_path(path, FLATTEN_STEPS);
            let mut inside: Vec<_> = samples
                .iter()
                .filter(|sample| point_in_rect(sample.point, rect))
                .map(|sample| sample.parameter)
                .collect();
            for pair in samples.windows(2) {
                for fraction in segment_rect_intersections(pair[0].point, pair[1].point, rect) {
                    inside.push(lerp(pair[0].parameter, pair[1].parameter, fraction));
                }
            }
            inside.sort_by(f64::total_cmp);
            let touched = !inside.is_empty();
            let all_inside = samples
                .iter()
                .all(|sample| point_in_rect(sample.point, rect));
            let range = match mode {
                VectorSelectionMode::FullyContained if all_inside => {
                    Some((0.0, path_length_t(path)))
                }
                VectorSelectionMode::CutBySelection if touched => inside
                    .first()
                    .zip(inside.last())
                    .map(|(start, end)| (*start, *end)),
                VectorSelectionMode::ToIntersection if touched => {
                    let touch = ((rect.0 + rect.2) * 0.5, (rect.1 + rect.3) * 0.5);
                    let touch_t = closest_path_parameter(path, touch, f32::MAX as f64)
                        .unwrap_or(path_length_t(path) * 0.5);
                    let mut intersections = document
                        .vector
                        .paths
                        .iter()
                        .filter(|other| other.id != path.id && other.plane_id == path.plane_id)
                        .flat_map(|other| path_intersections(path, other))
                        .collect::<Vec<_>>();
                    intersections.sort_by(f64::total_cmp);
                    Some((
                        intersections
                            .iter()
                            .copied()
                            .rfind(|value| *value < touch_t)
                            .unwrap_or(0.0),
                        intersections
                            .iter()
                            .copied()
                            .find(|value| *value > touch_t)
                            .unwrap_or(path_length_t(path)),
                    ))
                }
                VectorSelectionMode::Touching
                | VectorSelectionMode::Line
                | VectorSelectionMode::WholeLine
                    if touched =>
                {
                    Some((0.0, path_length_t(path)))
                }
                _ => None,
            };
            if let Some((start, end)) = range {
                result.path_ranges.push(selection_range(path, start, end));
            }
        }
        Ok(result)
    }
}
