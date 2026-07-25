use std::fs;
use std::path::Path;

fn collect_rust_sources(directory: &Path, output: &mut Vec<std::path::PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
        .map(|entry| entry.expect("failed to read source directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();

    for path in entries {
        if path.is_dir() {
            collect_rust_sources(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.push(path);
        }
    }
}

fn collect_crate_rust_sources(crate_root: &Path) -> Vec<std::path::PathBuf> {
    let mut sources = Vec::new();
    for directory in ["src", "tests", "benches", "examples"] {
        let path = crate_root.join(directory);
        if path.is_dir() {
            collect_rust_sources(&path, &mut sources);
        }
    }
    let build_script = crate_root.join("build.rs");
    if build_script.is_file() {
        sources.push(build_script);
    }
    sources.sort();
    sources
}

#[test]
fn arch_002_rust_domain_crates_do_not_reference_windows_apis() {
    let forbidden = [
        "HWND",
        "Win32",
        "winapi",
        "windows::",
        "windows_sys",
        "windows_core",
        "std::os::windows",
        "Direct2D",
        "Direct3D",
        "D3D11",
        "DXGI",
        "IUnknown",
        "HINSTANCE",
        "WPARAM",
        "LPARAM",
        "HRESULT",
        "WinRT",
        "DirectWrite",
        "HANDLE",
        "HDC",
        "LRESULT",
        "DWORD",
        "WIC",
        "Common Controls",
        "registry",
    ];

    let rust_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate must be below the Rust workspace directory");
    for crate_name in ["inkpod-core", "inkpod-image", "inkpod-format"] {
        let crate_root = rust_root.join(crate_name);
        let mut sources = Vec::new();
        collect_rust_sources(&crate_root.join("src"), &mut sources);
        for source in sources {
            let contents = fs::read_to_string(&source)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()));
            for token in forbidden {
                assert!(
                    !contents.contains(token),
                    "{} contains forbidden frontend token {token}",
                    source.display()
                );
            }
        }

        let manifest_path = crate_root.join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
        for token in [
            "windows =",
            "windows-sys",
            "windows-core",
            "windows-targets",
            "winapi =",
            "cfg(windows)",
            "target_os = \"windows\"",
        ] {
            assert!(
                !manifest.contains(token),
                "{} contains forbidden frontend dependency token {token}",
                manifest_path.display()
            );
        }
    }
}

#[test]
fn rust_crate_roots_remain_small_indices_and_tests_stay_out_of_src() {
    let rust_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate must be below the Rust workspace directory");
    for crate_name in ["inkpod-core", "inkpod-image", "inkpod-format", "inkpod-ffi"] {
        let crate_root = rust_root.join(crate_name);
        let source_root = crate_root.join("src");
        let library_path = source_root.join("lib.rs");
        let library = fs::read_to_string(&library_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", library_path.display()));
        assert!(
            library.lines().count() <= 200,
            "{} must remain a small module/re-export index",
            library_path.display()
        );

        let mut production_sources = Vec::new();
        collect_rust_sources(&source_root, &mut production_sources);
        for source in production_sources {
            let contents = fs::read_to_string(&source)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()));
            for inline_test_token in ["#[test]", "mod tests {"] {
                assert!(
                    !contents.contains(inline_test_token),
                    "{} contains inline test code; tests belong below {}/tests",
                    source.display(),
                    crate_root.display()
                );
            }
        }
    }

    let repository_root = rust_root
        .parent()
        .expect("Rust workspace must be below the repository root");
    let cmake_path = repository_root.join("CMakeLists.txt");
    let cmake = fs::read_to_string(&cmake_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", cmake_path.display()));
    assert!(
        cmake.contains("file(GLOB_RECURSE INKPOD_RUST_SOURCE_INPUTS CONFIGURE_DEPENDS"),
        "{} must recursively track Rust production sources",
        cmake_path.display()
    );
    for crate_name in ["inkpod-core", "inkpod-image", "inkpod-format", "inkpod-ffi"] {
        let expected = format!("rust/{crate_name}/src/*.rs");
        assert!(
            cmake.contains(&expected),
            "{} does not recursively track {expected}",
            cmake_path.display()
        );
    }
}

#[test]
fn public_rust_and_c_ffi_sources_do_not_expose_milestone_names() {
    let rust_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate must be below the Rust workspace directory");

    for crate_name in ["inkpod-core", "inkpod-image", "inkpod-format", "inkpod-ffi"] {
        let mut sources = Vec::new();
        collect_rust_sources(&rust_root.join(crate_name).join("src"), &mut sources);
        for source in sources {
            let file_name = source
                .file_name()
                .and_then(|name| name.to_str())
                .expect("Rust source file name must be UTF-8");
            let contents = fs::read_to_string(&source)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()));
            for milestone in 0..=9 {
                for forbidden_file_name in [
                    format!("m{milestone}.rs"),
                    format!("native_m{milestone}.rs"),
                ] {
                    assert_ne!(
                        file_name,
                        forbidden_file_name,
                        "{} exposes a milestone as a production module name",
                        source.display()
                    );
                }
                for forbidden_identifier in [
                    format!("pub struct FileM{milestone}"),
                    format!("pub enum FileM{milestone}"),
                    format!("pub type FileM{milestone}"),
                    format!("pub struct InkpodM{milestone}"),
                    format!("pub type InkpodM{milestone}"),
                    format!("pub const INKPOD_M{milestone}"),
                    format!("fn inkpod_m{milestone}"),
                    format!("mod m{milestone};"),
                ] {
                    assert!(
                        !contents.contains(&forbidden_identifier),
                        "{} exposes milestone identifier {forbidden_identifier}",
                        source.display()
                    );
                }
            }
        }
    }

    let repository_root = rust_root
        .parent()
        .expect("Rust workspace must be below the repository root");
    let header_path = repository_root.join("include/inkpod/core_ffi.h");
    let header = fs::read_to_string(&header_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", header_path.display()));
    for milestone in 0..=9 {
        for forbidden_identifier in [
            format!("InkpodM{milestone}"),
            format!("INKPOD_M{milestone}"),
            format!("inkpod_m{milestone}"),
        ] {
            assert!(
                !header.contains(&forbidden_identifier),
                "{} exposes milestone identifier {forbidden_identifier}",
                header_path.display()
            );
        }
    }
}

#[test]
fn acceptance_rust_workspace_has_zero_windows_imports() {
    let rust_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate must be below the Rust workspace directory");
    let forbidden_source_imports = [
        "windows::",
        "windows_sys::",
        "windows_core::",
        "externcratewindows",
        "std::os::windows",
        "cfg(windows)",
        "cfg!(windows)",
        "target_os=\"windows\"",
        "raw-dylib",
    ];
    let forbidden_packages = [
        "name = \"windows\"",
        "name = \"windows-sys\"",
        "name = \"windows-core\"",
        "name = \"windows-targets\"",
        "name = \"winapi\"",
    ];

    for crate_name in ["inkpod-core", "inkpod-image", "inkpod-format", "inkpod-ffi"] {
        let crate_root = rust_root.join(crate_name);
        for source in collect_crate_rust_sources(&crate_root) {
            if source.ends_with(Path::new("tests/architecture.rs")) {
                // This test necessarily contains the forbidden tokens as data.
                continue;
            }
            let contents = fs::read_to_string(&source)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()));
            let compact = contents
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>();
            for token in forbidden_source_imports {
                assert!(
                    !compact.contains(token),
                    "{} contains Windows-only Rust import/configuration {token}",
                    source.display()
                );
            }
        }

        let manifest_path = crate_root.join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
        let compact = manifest
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        for token in [
            "windows =",
            "windows-sys",
            "windows-core",
            "windows-targets",
            "winapi =",
            "cfg(windows)",
            "target_os=\"windows\"",
            "package=\"windows\"",
            "package=\"windows-sys\"",
            "package=\"windows-core\"",
            "package=\"winapi\"",
        ] {
            let token = token
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>();
            assert!(
                !compact.contains(&token),
                "{} contains Windows-only dependency/configuration {token}",
                manifest_path.display()
            );
        }
    }

    let workspace_manifest_path = rust_root
        .parent()
        .expect("Rust workspace must be below the repository root")
        .join("Cargo.toml");
    let workspace_manifest = fs::read_to_string(&workspace_manifest_path).unwrap_or_else(|error| {
        panic!(
            "failed to read {}: {error}",
            workspace_manifest_path.display()
        )
    });
    let compact_workspace_manifest = workspace_manifest
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    for token in [
        "windows=",
        "windows-sys",
        "windows-core",
        "windows-targets",
        "winapi=",
        "cfg(windows)",
        "target_os=\"windows\"",
        "package=\"windows\"",
        "package=\"windows-sys\"",
        "package=\"windows-core\"",
        "package=\"winapi\"",
    ] {
        assert!(
            !compact_workspace_manifest.contains(token),
            "{} contains Windows-only dependency/configuration {token}",
            workspace_manifest_path.display()
        );
    }

    let lock_path = rust_root
        .parent()
        .expect("Rust workspace must be below the repository root")
        .join("Cargo.lock");
    let lock = fs::read_to_string(&lock_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", lock_path.display()));
    for package in forbidden_packages {
        assert!(
            !lock.contains(package),
            "{} contains Windows-only package {package}",
            lock_path.display()
        );
    }
}

#[test]
fn acceptance_unverified_legacy_codecs_remain_unknown() {
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate must be below the repository root");
    let compatibility_path = repository_root.join("docs/compatibility.md");
    let compatibility = fs::read_to_string(&compatibility_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", compatibility_path.display()));
    for item in [
        "DGA binary codec",
        "CEL binary codec",
        "Legacy palette preset",
        "Legacy chart preset",
        "Legacy filter preset",
    ] {
        let row = compatibility
            .lines()
            .find(|line| line.starts_with(&format!("| {item} |")))
            .unwrap_or_else(|| panic!("compatibility matrix is missing {item}"));
        assert!(
            row.contains("| Unknown |")
                && row.contains("| 0 fixtures |")
                && row.contains("| 0 variants | 0 variants | 0 variants |"),
            "unverified legacy row must record zero measured read/write/round-trip scope: {row}"
        );
        assert!(
            !row.contains("| Verified |"),
            "unverified legacy codec must not be marked Verified: {row}"
        );
    }
}
