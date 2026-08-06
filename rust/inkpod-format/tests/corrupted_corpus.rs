use inkpod_format::{
    BATCH_GRAPH_VERSION, CommonRaster, CommonRasterFormat, FileBatchGraph, FileBatchInput,
    FileBatchOperation, FileBatchOutput, FileBatchTarget, NativeFile, NativeRecord, NativeSection,
    SECTION_CRITICAL, decode_batch_graph, decode_common_raster, decode_procedure_file,
    encode_batch_graph, encode_common_raster, encode_procedure_file, read_batch_graph,
    read_procedure_file,
};
use inkpod_image::PixelFormat;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug)]
enum Decoder {
    Native,
    Batch,
    Raster(CommonRasterFormat),
}

fn parse_hex(source: &str) -> Vec<u8> {
    source
        .lines()
        .map(|line| line.split('#').next().unwrap_or_default())
        .flat_map(str::split_whitespace)
        .map(|byte| u8::from_str_radix(byte, 16).expect("corpus byte must be hexadecimal"))
        .collect()
}

fn decode_without_panic(decoder: Decoder, bytes: &[u8]) -> Result<(), inkpod_format::FormatError> {
    catch_unwind(AssertUnwindSafe(|| match decoder {
        Decoder::Native => decode_procedure_file(bytes).map(|_| ()),
        Decoder::Batch => decode_batch_graph(bytes).map(|_| ()),
        Decoder::Raster(format) => decode_common_raster(format, bytes).map(|_| ()),
    }))
    .expect("corrupted input must never panic")
}

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn create() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock must be after the Unix epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("inkpod-test-corpus-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).expect("corpus temporary directory must be created");
        Self(path)
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn native_seed() -> Vec<u8> {
    let section = |fourcc| NativeSection {
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
    encode_procedure_file(&NativeFile {
        primitive_catalog_digest: [0x5a; 32],
        sections: [*b"META", *b"GENS", *b"ASST", *b"PROC", *b"EDIT"]
            .into_iter()
            .map(section)
            .collect(),
    })
    .expect("native mutation seed must encode")
}

fn batch_seed() -> Vec<u8> {
    encode_batch_graph(&FileBatchGraph {
        version: BATCH_GRAPH_VERSION,
        name: "corrupted-corpus".to_owned(),
        inputs: vec![FileBatchInput {
            kind: 1,
            path: "input.inkpod".to_owned(),
            first_cell: 1,
            last_cell: 1,
        }],
        operations: vec![FileBatchOperation {
            version: 1,
            kind: 1,
            flags: 1,
            target: FileBatchTarget::default(),
            payload: vec![1, 2, 3, 4],
        }],
        output: FileBatchOutput {
            policy: 2,
            folder: "output".to_owned(),
            cell_folder: false,
            format: 1,
            basename: "cell".to_owned(),
            start_number: 1,
            descending: false,
            failure_policy: 1,
            wait_milliseconds: 0,
            preview_before_save: false,
        },
    })
    .expect("batch mutation seed must encode")
}

fn raster_seed(format: CommonRasterFormat) -> Vec<u8> {
    let raster = CommonRaster::new(
        2,
        2,
        PixelFormat::StraightRgba8,
        Some(96_000),
        Some(96_000),
        vec![1, 2, 3, 255, 4, 5, 6, 128, 7, 8, 9, 0, 10, 11, 12, 255],
    )
    .expect("common-raster mutation seed must be valid");
    encode_common_raster(format, &raster, false).expect("common-raster mutation seed must encode")
}

#[test]
fn acceptance_corrupted_file_corpus_is_bounded_and_non_destructive() {
    let corpus = [
        (
            "native_manifest_overflow",
            Decoder::Native,
            include_str!("corpus/corrupted/native_manifest_overflow.hex"),
            "native header is truncated",
        ),
        (
            "batch_body_overflow",
            Decoder::Batch,
            include_str!("corpus/corrupted/batch_body_overflow.hex"),
            "batch graph is truncated",
        ),
        (
            "png_bad_oversized_ihdr",
            Decoder::Raster(CommonRasterFormat::Png),
            include_str!("corpus/corrupted/png_bad_oversized_ihdr.hex"),
            "common raster dimensions are outside bounds",
        ),
        (
            "tiff_ifd_count_overflow",
            Decoder::Raster(CommonRasterFormat::Tiff),
            include_str!("corpus/corrupted/tiff_ifd_count_overflow.hex"),
            "TIFF IFD entry count exceeds its bound",
        ),
        (
            "tga_dimension_overflow",
            Decoder::Raster(CommonRasterFormat::Tga),
            include_str!("corpus/corrupted/tga_dimension_overflow.hex"),
            "common raster byte length overflows",
        ),
        (
            "bmp_dimension_overflow",
            Decoder::Raster(CommonRasterFormat::Bmp),
            include_str!("corpus/corrupted/bmp_dimension_overflow.hex"),
            "BMP pixel byte length overflows",
        ),
    ];
    let directory = TemporaryDirectory::create();
    let protected_output = directory.0.join("existing-output.inkpod");
    let sentinel = b"existing output must survive corrupt input";
    fs::write(&protected_output, sentinel).expect("sentinel output must be written");

    for (name, decoder, source, expected_error) in corpus {
        let bytes = parse_hex(source);
        let input = directory.0.join(format!("{name}.corrupt"));
        fs::write(&input, &bytes).expect("corpus input must be written");
        let decode_error = decode_without_panic(decoder, &bytes)
            .expect_err("corrupted corpus entry must be rejected");
        assert!(
            decode_error.to_string().contains(expected_error),
            "{name} took the wrong bounded rejection path: {decode_error}"
        );
        let file_result = catch_unwind(AssertUnwindSafe(|| match decoder {
            Decoder::Native => read_procedure_file(&input).map(|_| ()),
            Decoder::Batch => read_batch_graph(&input).map(|_| ()),
            Decoder::Raster(format) => decode_common_raster(format, &bytes).map(|_| ()),
        }));
        assert!(file_result.is_ok(), "{name} file path panicked");
        let file_error = file_result
            .expect("checked above")
            .expect_err("corrupted file path must be rejected");
        assert!(
            file_error.to_string().contains(expected_error),
            "{name} file path took the wrong bounded rejection path: {file_error}"
        );
        assert_eq!(
            fs::read(&input).expect("corpus input must remain readable"),
            bytes,
            "{name} input changed"
        );
        assert_eq!(
            fs::read(&protected_output).expect("sentinel output must remain readable"),
            sentinel,
            "{name} overwrote an existing output"
        );
    }
    assert_eq!(
        fs::read_dir(&directory.0)
            .expect("temporary directory must remain readable")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp."))
            .count(),
        0,
        "corrupted decode left a temporary output"
    );
}

#[test]
fn mutation_fuzz_all_file_decoders_never_panics() {
    let mut seeds = vec![
        (Decoder::Native, native_seed()),
        (Decoder::Batch, batch_seed()),
    ];
    for format in [
        CommonRasterFormat::Png,
        CommonRasterFormat::Tiff,
        CommonRasterFormat::Tga,
        CommonRasterFormat::Bmp,
    ] {
        seeds.push((Decoder::Raster(format), raster_seed(format)));
    }

    for (decoder, seed) in seeds {
        for length in (0..seed.len()).step_by((seed.len() / 31).max(1)) {
            let truncated = &seed[..length];
            let _ = decode_without_panic(decoder, truncated);
        }
        for index in (0..seed.len()).step_by((seed.len() / 127).max(1)) {
            for mask in [0x01, 0x55, 0x80, 0xff] {
                let mut mutated = seed.clone();
                mutated[index] ^= mask;
                let result = catch_unwind(AssertUnwindSafe(|| match decoder {
                    Decoder::Native => decode_procedure_file(&mutated).map(|_| ()),
                    Decoder::Batch => decode_batch_graph(&mutated).map(|_| ()),
                    Decoder::Raster(format) => decode_common_raster(format, &mutated).map(|_| ()),
                }));
                assert!(
                    result.is_ok(),
                    "{decoder:?} panicked for mutation at byte {index} with mask {mask:#04x}"
                );
            }
        }
    }
}
