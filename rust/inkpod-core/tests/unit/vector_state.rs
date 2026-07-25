use super::*;

#[test]
fn vector_object_limits_cover_every_persisted_collection() {
    let state = VectorState::default();
    assert!(
        state
            .ensure_additional_limits(
                MAX_VECTOR_PATHS,
                MAX_VECTOR_FILLS,
                MAX_VECTOR_SEGMENTS,
                MAX_VECTOR_BOUNDARIES,
            )
            .is_ok()
    );
    for additions in [
        (MAX_VECTOR_PATHS + 1, 0, 0, 0),
        (0, MAX_VECTOR_FILLS + 1, 0, 0),
        (0, 0, MAX_VECTOR_SEGMENTS + 1, 0),
        (0, 0, 0, MAX_VECTOR_BOUNDARIES + 1),
    ] {
        assert!(matches!(
            state.ensure_additional_limits(additions.0, additions.1, additions.2, additions.3,),
            Err(CoreError::InvalidState("vector object limit reached"))
        ));
    }
    assert_eq!(state.raster_vectorize_run_capacity().unwrap(), 65_536);
}
