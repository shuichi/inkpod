use super::*;

fn base_fixture() -> DocumentArchive {
    DocumentArchive {
        document_uuid: [
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x10, 0x32, 0x54, 0x76, 0x98, 0xba,
            0xdc, 0xfe,
        ],
        document_id: 1,
        cell_id: 5,
        layer_id: 2,
        main_plane_id: 3,
        color_plane_id: 4,
        width: 65,
        height: 65,
        dpi_x_milli: 96_000,
        dpi_y_milli: 96_000,
        frames: FrameMetadata {
            hundred_frame: RectI32 {
                x: 0,
                y: 0,
                width: 65,
                height: 65,
            },
            reference_frame: RectI32 {
                x: 32,
                y: 32,
                width: 65,
                height: 65,
            },
            drawing_frame: RectI32 {
                x: 0,
                y: 0,
                width: 65,
                height: 65,
            },
            safe_frame: RectI32 {
                x: 3,
                y: 3,
                width: 59,
                height: 59,
            },
            shooting_frame: RectI32 {
                x: 0,
                y: 0,
                width: 65,
                height: 65,
            },
            maximum_close_frame: RectI32 {
                x: 0,
                y: 0,
                width: 65,
                height: 65,
            },
            margins: Margins::default(),
        },
        main_line_color: PixelValue::Rgba([0, 0, 0, 255]),
        palette: vec![
            PixelValue::Rgba([12, 34, 56, 255]),
            PixelValue::Rgba16([1, 257, 32_769, 65_534]),
        ],
        planes: vec![
            FilePlane {
                id: 3,
                kind: PlaneKind::MainLine,
                pixel_format: PixelFormat::BinaryMask8,
                width: 65,
                height: 65,
                tiles: vec![FileTile {
                    coord: TileCoord { x: 1, y: 1 },
                    width: 1,
                    height: 1,
                    bytes: vec![255],
                }],
            },
            FilePlane {
                id: 4,
                kind: PlaneKind::Color,
                pixel_format: PixelFormat::StraightRgba8,
                width: 65,
                height: 65,
                tiles: vec![FileTile {
                    coord: TileCoord { x: 1, y: 1 },
                    width: 1,
                    height: 1,
                    bytes: vec![1, 2, 3, 255],
                }],
            },
        ],
        document_metadata: None,
        light_table_metadata: None,
        vector_metadata: None,
        adjustment_metadata: None,
    }
}

fn document_tree_fixture() -> DocumentArchive {
    let mut document = base_fixture();
    document.planes.push(FilePlane {
        id: 5,
        kind: PlaneKind::Selection,
        pixel_format: PixelFormat::BinaryMask8,
        width: document.width,
        height: document.height,
        tiles: Vec::new(),
    });
    document.document_metadata = Some(FileDocumentMetadata {
        active_layer_id: 2,
        active_plane_id: 3,
        selection_plane_id: 5,
        layers: vec![FileLayer {
            id: 2,
            kind: LayerKind::BinaryColoring,
            name: "Coloring".to_owned(),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            planes: vec![
                FilePlaneProperties {
                    id: 3,
                    name: "Main".to_owned(),
                    visible: true,
                    editable: true,
                    opacity_milli: 1_000,
                },
                FilePlaneProperties {
                    id: 4,
                    name: "Color".to_owned(),
                    visible: true,
                    editable: true,
                    opacity_milli: 1_000,
                },
            ],
        }],
        guides: vec![FileGuide {
            id: 6,
            axis: GuideAxis::Vertical,
            position: 32,
        }],
        grid: FileGrid {
            origin_x: 0,
            origin_y: 0,
            spacing_x: 16,
            spacing_y: 16,
            subdivisions: 2,
        },
    });
    document
}

fn light_table_fixture() -> DocumentArchive {
    let mut document = document_tree_fixture();
    document.planes.push(FilePlane {
        id: 9,
        kind: PlaneKind::LightTable,
        pixel_format: PixelFormat::StraightRgba8,
        width: 4,
        height: 3,
        tiles: vec![FileTile {
            coord: TileCoord { x: 0, y: 0 },
            width: 4,
            height: 3,
            bytes: [10_u8, 20, 30, 255].repeat(12),
        }],
    });
    document.light_table_metadata = Some(FileLightTableMetadata {
        active_set_id: 7,
        sets: vec![FileLightTableSet {
            id: 7,
            name: "Default".to_owned(),
            global_opacity_milli: 500,
            items: vec![FileLightTableItem {
                id: 8,
                source_plane_id: 9,
                source_document_uuid: 0x1234_u128.to_le_bytes(),
                source_revision: 9,
                source_reference_frame: RectI32 {
                    x: 2,
                    y: 1,
                    width: 4,
                    height: 3,
                },
                source_dpi_x_milli: 96_000,
                source_dpi_y_milli: 96_000,
                name: "Reference".to_owned(),
                visible: true,
                opacity_milli: 500,
                display_mode: LightTableDisplayMode::Color,
                display_color: PixelValue::Rgba([0, 128, 255, 255]),
                translate_x_milli: 0,
                translate_y_milli: 0,
                scale_x_milli: 1_000,
                scale_y_milli: 1_000,
                rotation_milli_degrees: 0,
            }],
        }],
    });
    document
}

fn vector_fixture() -> DocumentArchive {
    let mut document = document_tree_fixture();
    for (id, kind) in [
        (8, PlaneKind::VectorMainLine),
        (9, PlaneKind::ColorTrace),
        (10, PlaneKind::VectorFill),
    ] {
        document.planes.push(FilePlane {
            id,
            kind,
            pixel_format: PixelFormat::StraightRgba8,
            width: document.width,
            height: document.height,
            tiles: Vec::new(),
        });
    }
    document
        .document_metadata
        .as_mut()
        .unwrap()
        .layers
        .push(FileLayer {
            id: 7,
            kind: LayerKind::VectorColoring,
            name: "Vector".to_owned(),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            planes: [(8, "Vector Main"), (9, "Color Trace"), (10, "Vector Fill")]
                .into_iter()
                .map(|(id, name)| FilePlaneProperties {
                    id,
                    name: name.to_owned(),
                    visible: true,
                    editable: true,
                    opacity_milli: 1_000,
                })
                .collect(),
        });
    let point = |x_milli, y_milli| FileVectorPoint { x_milli, y_milli };
    let line = |p0: FileVectorPoint, p3: FileVectorPoint| FileVectorSegment {
        p0,
        p1: FileVectorPoint {
            x_milli: (p0.x_milli * 2 + p3.x_milli) / 3,
            y_milli: (p0.y_milli * 2 + p3.y_milli) / 3,
        },
        p2: FileVectorPoint {
            x_milli: (p0.x_milli + p3.x_milli * 2) / 3,
            y_milli: (p0.y_milli + p3.y_milli * 2) / 3,
        },
        p3,
        width_start_milli: 1_000,
        width_end_milli: 2_000,
    };
    let corners = [
        point(1_000, 1_000),
        point(5_000, 1_000),
        point(5_000, 5_000),
        point(1_000, 5_000),
        point(1_000, 1_000),
    ];
    document.vector_metadata = Some(FileVectorMetadata {
        paths: vec![FileVectorPath {
            id: 11,
            plane_id: 9,
            color: PixelValue::Rgba16([1, 2, 3, 65_535]),
            closed: true,
            segments: corners
                .windows(2)
                .map(|pair| line(pair[0], pair[1]))
                .collect(),
        }],
        fills: vec![FileVectorFill {
            id: 12,
            plane_id: 10,
            color: PixelValue::Rgba([20, 40, 60, 200]),
            boundary_path_ids: vec![11],
        }],
    });
    document
}

#[test]
fn io_001_manifest_and_blobs_round_trip() {
    let document = base_fixture();
    let bytes = encode(&document).unwrap();
    assert_eq!(decode(&bytes).unwrap(), document);
}

#[test]
fn non_current_container_versions_are_rejected_before_format_freeze() {
    for version in [2_u32, 3_u32] {
        let mut encoded = encode(&base_fixture()).unwrap();
        encoded[8..12].copy_from_slice(&version.to_le_bytes());
        assert!(matches!(
            decode(&encoded),
            Err(FormatError::Unsupported("format version is not supported"))
        ));
    }
}

fn procedure_file_fixture() -> NativeFile {
    let singleton = |fourcc| NativeSection {
        fourcc,
        schema_version: 1,
        flags: SECTION_CRITICAL,
        records: vec![NativeRecord {
            kind: 1,
            schema_version: 1,
            flags: 0,
            payload: fourcc.to_vec(),
        }],
    };
    NativeFile {
        primitive_catalog_digest: [0x5a; 32],
        sections: vec![
            singleton(*b"PROC"),
            singleton(*b"EDIT"),
            singleton(*b"META"),
            NativeSection {
                fourcc: *b"ASST",
                schema_version: 1,
                flags: SECTION_CRITICAL,
                records: Vec::new(),
            },
            singleton(*b"GENS"),
            NativeSection {
                fourcc: *b"VEND",
                schema_version: 3,
                flags: OPAQUE_PRESERVE,
                records: vec![NativeRecord {
                    kind: 42,
                    schema_version: 7,
                    flags: 0x1020_3040,
                    payload: vec![9, 8, 7, 6],
                }],
            },
        ],
    }
}

#[test]
fn io_001_v14_directory_digest_and_opaque_sections_round_trip() {
    let file = procedure_file_fixture();
    let bytes = encode_procedure_file(&file).unwrap();
    assert_eq!(&bytes[0..8], b"INKPOD\0\0");
    assert_eq!(
        u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        FORMAT_VERSION
    );
    assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 8);
    assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 128);
    let mut expected = file;
    expected
        .sections
        .sort_by_key(|section| (section.fourcc, section.schema_version));
    assert_eq!(decode_procedure_file(&bytes).unwrap(), expected);

    let directory_offset = u64::from_le_bytes(bytes[32..40].try_into().unwrap()) as usize;
    let identities = bytes[directory_offset..]
        .chunks_exact(128)
        .map(|entry| {
            (
                entry[0..4].to_vec(),
                u16::from_le_bytes(entry[4..6].try_into().unwrap()),
            )
        })
        .collect::<Vec<_>>();
    assert!(identities.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn io_001_v14_accepts_checkpoint_and_rejects_v13_missing_duplicate_overlap_and_bad_digest() {
    let file = procedure_file_fixture();
    let encoded = encode_procedure_file(&file).unwrap();

    let mut v13 = encoded.clone();
    v13[8..12].copy_from_slice(&13_u32.to_le_bytes());
    assert!(matches!(
        decode_procedure_file(&v13),
        Err(FormatError::Unsupported("format version is not supported"))
    ));

    let mut missing = file.clone();
    missing
        .sections
        .retain(|section| section.fourcc != *b"GENS");
    assert!(matches!(
        encode_procedure_file(&missing),
        Err(FormatError::Invalid("required native section is missing"))
    ));

    let mut duplicate = file.clone();
    duplicate.sections.push(duplicate.sections[0].clone());
    assert!(matches!(
        encode_procedure_file(&duplicate),
        Err(FormatError::Invalid("native section is duplicated"))
    ));

    let mut checkpoint = file.clone();
    checkpoint.sections.push(NativeSection {
        fourcc: *b"CKPT",
        schema_version: 1,
        flags: 0,
        records: vec![NativeRecord {
            kind: 1,
            schema_version: 1,
            flags: 0,
            payload: Vec::new(),
        }],
    });
    let checkpoint_bytes = encode_procedure_file(&checkpoint).unwrap();
    assert!(
        decode_procedure_file(&checkpoint_bytes)
            .unwrap()
            .sections
            .iter()
            .any(|section| section.fourcc == *b"CKPT")
    );

    let directory_offset = u64::from_le_bytes(encoded[32..40].try_into().unwrap()) as usize;
    let mut overlap = encoded.clone();
    let edit_offset = overlap[directory_offset + 128 + 16..directory_offset + 128 + 24].to_vec();
    overlap[directory_offset + 256 + 16..directory_offset + 256 + 24].copy_from_slice(&edit_offset);
    assert!(decode_procedure_file(&overlap).is_err());

    let mut bad_digest = encoded;
    bad_digest[80] ^= 1;
    assert!(matches!(
        decode_procedure_file(&bad_digest),
        Err(FormatError::ChecksumMismatch)
    ));
}

#[test]
fn io_001_v14_streaming_cancel_keeps_existing_destination_and_removes_temp() {
    let directory = std::env::temp_dir().join(format!(
        "inkpod-v14-cancel-test-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("cell.inkpod");
    fs::write(&path, b"original").unwrap();
    let mut file = procedure_file_fixture();
    file.sections
        .iter_mut()
        .find(|section| section.fourcc == *b"VEND")
        .unwrap()
        .records[0]
        .payload = vec![0x5a; 2_500_000];
    let mut checks = 0;
    let result = save_procedure_file_atomic_with_cancel(&path, &file, || {
        checks += 1;
        checks == 3
    });
    assert!(matches!(result, Err(FormatError::Cancelled)));
    assert_eq!(fs::read(&path).unwrap(), b"original");
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
    fs::remove_file(path).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn io_001_v14_atomic_save_replaces_an_existing_container() {
    let directory = std::env::temp_dir().join(format!(
        "inkpod-v14-replace-test-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("cell.inkpod");
    let first = procedure_file_fixture();
    save_procedure_file_atomic(&path, &first).unwrap();
    let mut second = first;
    second
        .sections
        .iter_mut()
        .find(|section| section.fourcc == *b"VEND")
        .unwrap()
        .records[0]
        .payload = vec![1, 2, 3, 4];
    save_procedure_file_atomic(&path, &second).unwrap();
    let mut expected = second;
    expected
        .sections
        .sort_by_key(|section| (section.fourcc, section.schema_version));
    assert_eq!(read_procedure_file(&path).unwrap(), expected);
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
    fs::remove_file(path).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn color_metadata_round_trips() {
    let mut document = base_fixture();
    document.main_line_color = PixelValue::Rgba16([1_001, 2_002, 3_003, 65_535]);
    let decoded = decode(&encode(&document).unwrap()).unwrap();
    assert_eq!(decoded.main_line_color, document.main_line_color);
    assert_eq!(decoded.palette, document.palette);
}

#[test]
fn grayscale_and_rgba16_tiles_round_trip_without_quantization() {
    let mut document = base_fixture();
    document.planes[0].pixel_format = PixelFormat::Grayscale16;
    document.planes[0].tiles[0].bytes = 0x1234_u16.to_le_bytes().to_vec();
    document.planes[1].pixel_format = PixelFormat::StraightRgba16;
    let exact = [1_u16, 257, 32_769, 65_534];
    document.planes[1].tiles[0].bytes = exact.into_iter().flat_map(u16::to_le_bytes).collect();

    let decoded = decode(&encode(&document).unwrap()).unwrap();
    assert_eq!(decoded, document);
    assert_eq!(decoded.planes[1].tiles[0].bytes[0..2], [1, 0]);
}

#[test]
fn document_metadata_rejects_out_of_bounds_guides_grid_and_unreferenced_payloads() {
    let document = document_tree_fixture();
    assert_eq!(decode(&encode(&document).unwrap()).unwrap(), document);

    let mut invalid_guide = document.clone();
    invalid_guide.document_metadata.as_mut().unwrap().guides[0].position = 66;
    assert!(matches!(
        encode(&invalid_guide),
        Err(FormatError::Invalid(
            "guide position is outside the document"
        ))
    ));

    let mut invalid_grid = document.clone();
    invalid_grid
        .document_metadata
        .as_mut()
        .unwrap()
        .grid
        .spacing_x = 1_048_577;
    assert!(matches!(
        encode(&invalid_grid),
        Err(FormatError::Invalid(
            "document metadata values are outside bounds"
        ))
    ));

    let mut unreferenced = document;
    let mut extra = unreferenced.planes[1].clone();
    extra.id = 7;
    unreferenced.planes.push(extra);
    assert!(matches!(
        encode(&unreferenced),
        Err(FormatError::Invalid(
            "document layer tree and plane payload IDs differ"
        ))
    ));
}

#[test]
fn light_table_metadata_round_trips_and_rejects_malformed_source_relationships() {
    let document = light_table_fixture();
    let encoded = encode(&document).unwrap();
    assert_eq!(decode(&encoded).unwrap(), document);

    let mut invalid_opacity = light_table_fixture();
    invalid_opacity.light_table_metadata.as_mut().unwrap().sets[0].items[0].opacity_milli = 1_001;
    assert!(matches!(
        encode(&invalid_opacity),
        Err(FormatError::Invalid(_))
    ));

    let mut missing_source = light_table_fixture();
    missing_source.planes.retain(|plane| plane.id != 9);
    assert!(matches!(
        encode(&missing_source),
        Err(FormatError::Invalid(_))
    ));

    let mut colliding_source = light_table_fixture();
    colliding_source
        .planes
        .iter_mut()
        .find(|plane| plane.kind == PlaneKind::LightTable)
        .unwrap()
        .id = 6;
    colliding_source.light_table_metadata.as_mut().unwrap().sets[0].items[0].source_plane_id = 6;
    assert!(matches!(
        encode(&colliding_source),
        Err(FormatError::Invalid(
            "light-table source plane collides with document state"
        ))
    ));

    let mut minimum_rotation = light_table_fixture();
    minimum_rotation.light_table_metadata.as_mut().unwrap().sets[0].items[0]
        .rotation_milli_degrees = i32::MIN;
    assert!(matches!(
        encode(&minimum_rotation),
        Err(FormatError::Invalid(
            "light-table item properties are invalid"
        ))
    ));

    let mut no_tree = light_table_fixture();
    no_tree.document_metadata = None;
    assert!(matches!(encode(&no_tree), Err(FormatError::Invalid(_))));
}

#[test]
fn vector_metadata_round_trips_and_rejects_malformed_topology() {
    let document = vector_fixture();
    assert_eq!(decode(&encode(&document).unwrap()).unwrap(), document);

    let mut missing_boundary = vector_fixture();
    missing_boundary.vector_metadata.as_mut().unwrap().fills[0].boundary_path_ids[0] = 99;
    assert!(matches!(
        encode(&missing_boundary),
        Err(FormatError::Invalid(_))
    ));

    let mut open_boundary = vector_fixture();
    open_boundary.vector_metadata.as_mut().unwrap().paths[0].closed = false;
    assert!(matches!(
        encode(&open_boundary),
        Err(FormatError::Invalid(_))
    ));

    let mut cross_layer = vector_fixture();
    for (id, kind) in [
        (14, PlaneKind::VectorMainLine),
        (15, PlaneKind::ColorTrace),
        (16, PlaneKind::VectorFill),
    ] {
        cross_layer.planes.push(FilePlane {
            id,
            kind,
            pixel_format: PixelFormat::StraightRgba8,
            width: cross_layer.width,
            height: cross_layer.height,
            tiles: Vec::new(),
        });
    }
    cross_layer
        .document_metadata
        .as_mut()
        .unwrap()
        .layers
        .push(FileLayer {
            id: 13,
            kind: LayerKind::VectorColoring,
            name: "Other vector".to_owned(),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            planes: [(14, "Main"), (15, "Trace"), (16, "Fill")]
                .into_iter()
                .map(|(id, name)| FilePlaneProperties {
                    id,
                    name: name.to_owned(),
                    visible: true,
                    editable: true,
                    opacity_milli: 1_000,
                })
                .collect(),
        });
    cross_layer.vector_metadata.as_mut().unwrap().fills[0].plane_id = 16;
    assert!(matches!(
        encode(&cross_layer),
        Err(FormatError::Invalid(
            "vector fill boundary crosses vector layers"
        ))
    ));

    let mut out_of_bounds = vector_fixture();
    out_of_bounds.vector_metadata.as_mut().unwrap().paths[0].segments[0]
        .p1
        .x_milli = i32::MAX;
    assert!(matches!(
        encode(&out_of_bounds),
        Err(FormatError::Invalid(
            "vector segment coordinate is outside bounds"
        ))
    ));

    let mut colliding_path = vector_fixture();
    colliding_path.vector_metadata.as_mut().unwrap().paths[0].id = 6;
    colliding_path.vector_metadata.as_mut().unwrap().fills[0].boundary_path_ids[0] = 6;
    assert!(matches!(
        encode(&colliding_path),
        Err(FormatError::Invalid(
            "vector path collides with an existing stable ID"
        ))
    ));

    let mut missing_metadata = vector_fixture();
    missing_metadata.vector_metadata = None;
    assert!(matches!(
        encode(&missing_metadata),
        Err(FormatError::Invalid(_))
    ));
}

#[test]
fn io_001_rejects_truncation_and_checksum_mismatch() {
    let mut bytes = encode(&base_fixture()).unwrap();
    assert!(decode(&bytes[..bytes.len() - 1]).is_err());
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    assert!(matches!(decode(&bytes), Err(FormatError::ChecksumMismatch)));
    let mut trailing = encode(&base_fixture()).unwrap();
    trailing.push(0);
    assert!(decode(&trailing).is_err());
}

#[test]
fn adjustment_metadata_round_trips_and_rejects_malformed_relationships() {
    let mut document = document_tree_fixture();
    document.document_metadata.as_mut().unwrap().layers.insert(
        0,
        FileLayer {
            id: 100,
            kind: LayerKind::Adjustment,
            name: "Adjustment".to_owned(),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            planes: Vec::new(),
        },
    );
    document.adjustment_metadata = Some(FileAdjustmentMetadata {
        adjustments: vec![FileAdjustmentLayer {
            layer_id: 100,
            adjustment: inkpod_image::Adjustment::BrightnessContrast {
                brightness_milli: 125,
                contrast_milli: -250,
            },
        }],
    });
    assert_eq!(decode(&encode(&document).unwrap()).unwrap(), document);

    let mut missing = document.clone();
    missing.adjustment_metadata = None;
    assert!(matches!(
        encode(&missing),
        Err(FormatError::Invalid(
            "adjustment layers require adjustment metadata"
        ))
    ));

    let mut duplicate = document.clone();
    let duplicate_adjustment =
        duplicate.adjustment_metadata.as_ref().unwrap().adjustments[0].clone();
    duplicate
        .adjustment_metadata
        .as_mut()
        .unwrap()
        .adjustments
        .push(duplicate_adjustment);
    assert!(matches!(
        encode(&duplicate),
        Err(FormatError::Invalid("adjustment properties are invalid"))
    ));

    let mut wrong_layer = document.clone();
    wrong_layer
        .adjustment_metadata
        .as_mut()
        .unwrap()
        .adjustments[0]
        .layer_id = 101;
    assert!(matches!(
        encode(&wrong_layer),
        Err(FormatError::Invalid("adjustment properties are invalid"))
    ));

    let mut invalid_parameter = document;
    invalid_parameter
        .adjustment_metadata
        .as_mut()
        .unwrap()
        .adjustments[0]
        .adjustment = inkpod_image::Adjustment::BrightnessContrast {
        brightness_milli: 1_001,
        contrast_milli: 0,
    };
    assert!(matches!(
        encode(&invalid_parameter),
        Err(FormatError::Invalid("adjustment properties are invalid"))
    ));
}

#[test]
fn io_001_atomic_save_cancel_keeps_existing_destination() {
    let directory = std::env::temp_dir().join(format!(
        "inkpod-format-test-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("cell.inkpod");
    fs::write(&path, b"original").unwrap();
    let mut checks = 0;
    let result = save_atomic_with_cancel(&path, &base_fixture(), || {
        checks += 1;
        checks == 2
    });
    assert!(matches!(result, Err(FormatError::Cancelled)));
    assert_eq!(fs::read(&path).unwrap(), b"original");
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
    fs::remove_file(&path).unwrap();
    fs::remove_dir(&directory).unwrap();
}

#[test]
fn io_001_atomic_save_replaces_an_existing_container() {
    let directory = std::env::temp_dir().join(format!(
        "inkpod-format-replace-test-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("cell.inkpod");
    let first = base_fixture();
    save_atomic(&path, &first).unwrap();
    let mut second = first.clone();
    second.planes[1].tiles[0].bytes = vec![9, 8, 7, 255];
    save_atomic(&path, &second).unwrap();
    assert_eq!(read(&path).unwrap(), second);
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
    fs::remove_file(&path).unwrap();
    fs::remove_dir(&directory).unwrap();
}
