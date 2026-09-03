use std::fs;
use std::path::Path;

const RUST_CRATES: [&str; 5] = [
    "inkpod-core",
    "inkpod-image",
    "inkpod-format",
    "inkpod-io",
    "inkpod-ffi",
];

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

fn collect_semantically_named_sources(directory: &Path, output: &mut Vec<std::path::PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
        .map(|entry| entry.expect("failed to read repository entry").path())
        .collect::<Vec<_>>();
    entries.sort();

    for path in entries {
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if name == ".git" || name == "target" || name == "out" || name.starts_with("build") {
                continue;
            }
            collect_semantically_named_sources(&path, output);
            continue;
        }

        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if name == "CMakeLists.txt"
            || matches!(
                extension,
                "c" | "cmake"
                    | "cpp"
                    | "h"
                    | "in"
                    | "inc"
                    | "json"
                    | "manifest"
                    | "ps1"
                    | "rc"
                    | "rs"
                    | "toml"
                    | "yaml"
                    | "yml"
            )
        {
            output.push(path);
        }
    }
}

fn temporary_phase_label(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    for offset in 0..bytes.len().saturating_sub(1) {
        let prefix = bytes[offset];
        let digit = bytes[offset + 1];
        let upper_bound = if prefix == b'M' { b'8' } else { b'6' };
        if matches!(prefix, b'M' | b'R') && (b'0'..=upper_bound).contains(&digit) {
            let preceded_by_uppercase = offset > 0 && bytes[offset - 1].is_ascii_uppercase();
            if !preceded_by_uppercase {
                return Some(text[offset..offset + 2].to_owned());
            }
        }

        if prefix == b'm' && (b'0'..=b'8').contains(&digit) {
            let preceded_by_alphanumeric = offset > 0 && bytes[offset - 1].is_ascii_alphanumeric();
            let followed_by_separator = bytes
                .get(offset + 2)
                .is_some_and(|value| matches!(value, b'_' | b'-'));
            if !preceded_by_alphanumeric && followed_by_separator {
                return Some(text[offset..offset + 2].to_owned());
            }
        }
    }
    None
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
fn rust_crate_roots_remain_small_indices_and_cmake_tracks_sources() {
    let rust_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate must be below the Rust workspace directory");
    for crate_name in RUST_CRATES {
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
        let syntax = syn::parse_file(&library).expect("crate root must parse as Rust");
        assert!(
            !syntax
                .items
                .iter()
                .any(|item| matches!(item, syn::Item::Fn(_) | syn::Item::Impl(_))),
            "{} must declare modules and exports, not production logic",
            library_path.display()
        );
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
    for crate_name in RUST_CRATES {
        let expected = format!("rust/{crate_name}/src/*.rs");
        assert!(
            cmake.contains(&expected),
            "{} does not recursively track {expected}",
            cmake_path.display()
        );
    }
}

#[test]
fn core_responsibility_modules_remain_split_and_declarative() {
    let core_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_root = core_root.join("src");

    for legacy_path in ["batch/codec.rs", "transform.rs", "view.rs"] {
        assert!(
            !source_root.join(legacy_path).exists(),
            "legacy Core monolith must not return: {legacy_path}"
        );
    }

    for module_path in [
        "batch/codec/codes.rs",
        "batch/codec/operation.rs",
        "batch/codec/filter.rs",
        "batch/codec/payload.rs",
        "transform/document.rs",
        "transform/raster.rs",
        "transform/frame.rs",
        "transform/numeric.rs",
        "view/commands.rs",
        "view/coordinates.rs",
        "view/guides.rs",
        "view/secondary.rs",
        "view/shortcuts.rs",
    ] {
        assert!(
            source_root.join(module_path).is_file(),
            "Core responsibility module is missing: {module_path}"
        );
    }

    for index_path in ["batch/codec/mod.rs", "transform/mod.rs", "view/mod.rs"] {
        let path = source_root.join(index_path);
        let contents = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert!(
            !contents.contains("impl Core")
                && !contents.lines().any(|line| {
                    let line = line.trim_start();
                    line.starts_with("fn ")
                        || line.starts_with("pub fn ")
                        || line.starts_with("pub(super) fn ")
                        || line.starts_with("pub(crate) fn ")
                        || line.starts_with("pub(in ") && line.contains(" fn ")
                }),
            "{} must remain a declarative module index",
            path.display()
        );
    }
}

#[test]
fn inline_test_modules_are_cfg_test_gated() {
    let rust_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate must be below the Rust workspace directory");
    for crate_name in RUST_CRATES {
        let source_root = rust_root.join(crate_name).join("src");
        let mut production_sources = Vec::new();
        collect_rust_sources(&source_root, &mut production_sources);
        for source in production_sources {
            let contents = fs::read_to_string(&source)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()));
            let lines = contents.lines().collect::<Vec<_>>();
            for (index, line) in lines.iter().enumerate() {
                if !line.trim_start().starts_with("mod tests") {
                    continue;
                }
                assert!(
                    lines[..index]
                        .iter()
                        .rev()
                        .take(3)
                        .any(|candidate| candidate.trim() == "#[cfg(test)]"),
                    "{} declares a test module without #[cfg(test)]",
                    source.display()
                );
            }
        }
    }
}

#[test]
fn production_and_test_identifiers_are_semantic() {
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate must be below the repository root");
    let mut sources = Vec::new();
    collect_semantically_named_sources(repository_root, &mut sources);

    for source in sources {
        let file_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .expect("source file name must be UTF-8");
        assert!(
            temporary_phase_label(file_name).is_none(),
            "{} uses a temporary phase label as a file name",
            source.display()
        );

        if source.ends_with(Path::new("schemas/inkscript/owner-manifest-v2.json"))
            || source.ends_with(Path::new("schemas/inkscript/catalog-v2.json"))
            || source.ends_with(Path::new("schemas/inkscript/owner-manifest-v3.json"))
            || source.ends_with(Path::new("schemas/inkscript/catalog-v3.json"))
            || source.ends_with(Path::new("schemas/inkscript/owner-manifest-v5.json"))
            || source.ends_with(Path::new("schemas/inkscript/catalog-v5.json"))
            || source.ends_with(Path::new("schemas/inkscript/owner-manifest-v6.json"))
            || source.ends_with(Path::new("schemas/inkscript/catalog-v6.json"))
        {
            // The InkScript ownership and production catalog registries intentionally retain
            // stable owner-milestone and equivalence-evidence IDs. Separate integration tests
            // prove their exact bijection with every replayable primitive and runtime adapter.
            continue;
        }

        let contents = fs::read_to_string(&source)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()));
        assert!(
            temporary_phase_label(&contents).is_none(),
            "{} contains temporary phase label {}",
            source.display(),
            temporary_phase_label(&contents).unwrap_or_default()
        );
    }
}

#[test]
fn acceptance_rust_windows_imports_are_confined_to_private_file_backend() {
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

    for crate_name in RUST_CRATES {
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
            let relative = source
                .strip_prefix(&crate_root)
                .expect("source must remain in its crate");
            let private_file_backend = crate_name == "inkpod-io"
                && [
                    Path::new("src/backend.rs"),
                    Path::new("src/backend/windows.rs"),
                ]
                .contains(&relative);
            let case_identity_test =
                crate_name == "inkpod-io" && relative == Path::new("tests/manager.rs");
            for token in forbidden_source_imports {
                // The approved OS exception is private filesystem identity and
                // publication only. No GUI, COM, renderer, or crate dependency
                // exception is granted by these exact file/token allowlists.
                if private_file_backend
                    && matches!(
                        token,
                        "windows::" | "std::os::windows" | "cfg(windows)" | "cfg!(windows)"
                    )
                {
                    continue;
                }
                if case_identity_test && token == "cfg(windows)" {
                    continue;
                }
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
fn io_backend_remains_private_and_cannot_import_domain_or_frontend_state() {
    let rust_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let io_root = rust_root.join("inkpod-io");
    let source_root = io_root.join("src");
    let mut sources = Vec::new();
    collect_rust_sources(&source_root, &mut sources);
    for source in sources {
        let contents = fs::read_to_string(&source).unwrap();
        for token in [
            "HWND",
            "HINSTANCE",
            "WPARAM",
            "LPARAM",
            "IUnknown",
            "WinRT",
            "Direct2D",
            "Direct3D",
            "DirectWrite",
            "D3D11",
            "DXGI",
            "WIC",
            "Common Controls",
            "inkpod_core",
            "core_ffi",
            "DocumentSession",
            "WorkspaceWindow",
        ] {
            assert!(
                !contents.contains(token),
                "{} contains forbidden IO backend token {token}",
                source.display()
            );
        }
        if source != source_root.join("backend/windows.rs") {
            for token in ["unsafe {", "unsafe fn ", "unsafe extern "] {
                assert!(
                    !contents.contains(token),
                    "{} contains unsafe outside the private filesystem ABI",
                    source.display()
                );
            }
        }
        if source
            .file_name()
            .is_some_and(|name| name == "lib.rs" || name == "mod.rs")
        {
            let syntax = syn::parse_file(&contents).unwrap();
            assert!(
                syntax
                    .items
                    .iter()
                    .all(|item| matches!(item, syn::Item::Use(_) | syn::Item::Mod(_))),
                "{} must remain a module/re-export index",
                source.display()
            );
        }
    }
    let manifest = fs::read_to_string(io_root.join("Cargo.toml")).unwrap();
    assert!(
        !manifest.contains("inkpod-core") && !manifest.contains("inkpod-ffi"),
        "filesystem ownership must not depend on a document or ABI crate"
    );
    let library = fs::read_to_string(source_root.join("lib.rs")).unwrap();
    assert!(library.lines().any(|line| line == "mod backend;"));
    assert!(!library.contains("pub mod backend") && !library.contains("pub use backend::*"));
}
