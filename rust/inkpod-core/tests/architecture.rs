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
