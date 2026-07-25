use inkpod_core::{Core, DEFAULT_DPI_MILLI};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn create() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock must be after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "inkpod-m8-core-open-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("M8 Core temporary directory must be created");
        Self(path)
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn parse_hex(source: &str) -> Vec<u8> {
    source
        .lines()
        .map(|line| line.split('#').next().unwrap_or_default())
        .flat_map(str::split_whitespace)
        .map(|byte| u8::from_str_radix(byte, 16).expect("corpus byte must be hexadecimal"))
        .collect()
}

#[test]
fn m8_corrupted_open_preserves_the_current_document_and_every_file() {
    let directory = TemporaryDirectory::create();
    let normal_path = directory.0.join("current.inkpod");
    let corrupt_path = directory.0.join("corrupt.inkpod");

    let mut core = Core::new();
    core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
        .expect("current cell must be created");
    core.save(&normal_path).expect("current cell must be saved");
    let before_info = core
        .document_info()
        .expect("current document info must be available");
    let before_normal = fs::read(&normal_path).expect("normal file must be readable");
    let corrupt = parse_hex(include_str!(
        "../../inkpod-format/tests/corpus/m8/native_manifest_overflow.hex"
    ));
    fs::write(&corrupt_path, &corrupt).expect("corrupt corpus file must be written");

    assert!(core.open(&corrupt_path).is_err());
    assert_eq!(
        core.document_info()
            .expect("current document must survive failed open"),
        before_info
    );
    assert_eq!(
        fs::read(&normal_path).expect("normal file must remain readable"),
        before_normal
    );
    assert_eq!(
        fs::read(&corrupt_path).expect("corrupt input must remain readable"),
        corrupt
    );
    assert_eq!(
        fs::read_dir(&directory.0)
            .expect("temporary directory must remain readable")
            .count(),
        2,
        "failed open must not create a temporary or output file"
    );
}
