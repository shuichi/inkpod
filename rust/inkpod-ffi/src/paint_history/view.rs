use super::*;

pub(crate) fn parse_view_command(core: &Core, input: &InkpodViewInput) -> Result<ViewCommand, u32> {
    if input.flags != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "view input contains unsupported flags",
        ));
    }
    let command = match input.kind {
        INKPOD_VIEW_PAN_BY => ViewCommand::PanBy {
            device_dx: input.value1,
            device_dy: input.value2,
        },
        INKPOD_VIEW_ZOOM_AT => ViewCommand::ZoomAt {
            factor: input.value1,
            device_x: input.value2,
            device_y: input.value3,
        },
        INKPOD_VIEW_FIT => ViewCommand::Fit {
            viewport_width: input.value1,
            viewport_height: input.value2,
        },
        INKPOD_VIEW_ONE_TO_ONE => ViewCommand::OneToOne {
            viewport_width: input.value1,
            viewport_height: input.value2,
        },
        INKPOD_VIEW_VIEWPORT_RESIZED => ViewCommand::ViewportResized {
            viewport_width: input.value1,
            viewport_height: input.value2,
        },
        INKPOD_VIEW_BOX_ZOOM => {
            if !input.value1.is_finite()
                || !input.value2.is_finite()
                || !input.value3.is_finite()
                || !input.value4.is_finite()
                || input.value1 < f64::from(i32::MIN)
                || input.value1 > f64::from(i32::MAX)
                || input.value2 < f64::from(i32::MIN)
                || input.value2 > f64::from(i32::MAX)
                || input.value3 < f64::from(i32::MIN)
                || input.value3 > f64::from(i32::MAX)
                || input.value4 < f64::from(i32::MIN)
                || input.value4 > f64::from(i32::MAX)
            {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "box zoom rectangle is outside the document coordinate range",
                ));
            }
            let view = core.view_state();
            ViewCommand::BoxZoom {
                document_rect: RectI32 {
                    x: input.value1 as i32,
                    y: input.value2 as i32,
                    width: input.value3 as i32,
                    height: input.value4 as i32,
                },
                viewport_width: view.viewport_width(),
                viewport_height: view.viewport_height(),
            }
        }
        INKPOD_VIEW_FLIP_HORIZONTAL => ViewCommand::Flip {
            axis: MirrorAxis::Horizontal,
        },
        INKPOD_VIEW_FLIP_VERTICAL => ViewCommand::Flip {
            axis: MirrorAxis::Vertical,
        },
        INKPOD_VIEW_SET_RULER_VISIBLE => ViewCommand::SetRulerVisible(input.value1 != 0.0),
        INKPOD_VIEW_SET_GUIDES_VISIBLE => ViewCommand::SetGuidesVisible(input.value1 != 0.0),
        INKPOD_VIEW_SET_GRID_VISIBLE => ViewCommand::SetGridVisible(input.value1 != 0.0),
        INKPOD_VIEW_SET_SNAP_ENABLED => ViewCommand::SetSnapEnabled(input.value1 != 0.0),
        INKPOD_VIEW_SET_GUIDE_SNAP_ENABLED => ViewCommand::SetGuideSnapEnabled(input.value1 != 0.0),
        INKPOD_VIEW_SET_GRID_SNAP_ENABLED => ViewCommand::SetGridSnapEnabled(input.value1 != 0.0),
        INKPOD_VIEW_SET_TRANSPARENT_VISIBLE => ViewCommand::SetTransparentView(input.value1 != 0.0),
        INKPOD_VIEW_SET_ALPHA_VISIBLE => ViewCommand::SetAlphaView(input.value1 != 0.0),
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "view command kind is not defined",
            ));
        }
    };
    Ok(command)
}
