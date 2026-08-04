use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use syn::{FnArg, ImplItem, Item, Signature, Type, Visibility};

const ROUTE_CATEGORIES: [&str; 8] = [
    "document-primitive",
    "history-control-event",
    "editor-state-command",
    "view-only-command",
    "transient-preview-stroke",
    "query-snapshot",
    "asset-data-plane",
    "os-application-adapter",
];

const ROUTE_OWNERS: [&str; 5] = [
    "rust-core",
    "rust-image",
    "rust-format",
    "rust-ffi-adapter",
    "windows-adapter",
];

#[derive(Debug)]
struct Route<'a> {
    surface: &'a str,
    category: &'a str,
    owner: &'a str,
    symbol: &'a str,
}

fn collect_rust_sources(directory: &Path, output: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
        .map(|entry| {
            entry
                .expect("source directory entry must be readable")
                .path()
        })
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

fn is_core_type(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    path.qself.is_none()
        && path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Core")
}

fn simple_type_name(ty: &Type) -> Option<String> {
    let Type::Path(path) = ty else {
        return None;
    };
    if path.qself.is_some() {
        return None;
    }
    path.path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn type_contains_path(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .iter()
            .any(|segment| segment.ident == "Path"),
        Type::Reference(reference) => type_contains_path(&reference.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(type_contains_path),
        _ => false,
    }
}

fn signature_has_path(signature: &Signature) -> bool {
    signature
        .inputs
        .iter()
        .any(|input| matches!(input, FnArg::Typed(argument) if type_contains_path(&argument.ty)))
}

fn signature_has_mutable_reference(signature: &Signature) -> bool {
    signature.inputs.iter().any(|input| {
        matches!(input, FnArg::Typed(argument) if matches!(argument.ty.as_ref(), Type::Reference(reference) if reference.mutability.is_some()))
    })
}

fn public_core_methods(source_root: &Path) -> BTreeSet<String> {
    let mut sources = Vec::new();
    collect_rust_sources(source_root, &mut sources);
    let mut methods = BTreeSet::new();
    for path in sources {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let syntax = syn::parse_file(&source)
            .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
        for item in syntax.items {
            let Item::Impl(item_impl) = item else {
                continue;
            };
            if item_impl.trait_.is_some() || !is_core_type(&item_impl.self_ty) {
                continue;
            }
            for item in item_impl.items {
                let ImplItem::Fn(method) = item else {
                    continue;
                };
                if matches!(method.vis, Visibility::Public(_)) {
                    assert!(
                        methods.insert(format!("Core::{}", method.sig.ident)),
                        "duplicate public Core method {}",
                        method.sig.ident
                    );
                }
            }
        }
    }
    methods
}

fn public_non_core_mutations(repository: &Path) -> BTreeSet<String> {
    let mut mutations = BTreeSet::new();
    for (crate_name, relative_root) in [
        ("inkpod_core", "rust/inkpod-core/src"),
        ("inkpod_image", "rust/inkpod-image/src"),
    ] {
        let mut sources = Vec::new();
        collect_rust_sources(&repository.join(relative_root), &mut sources);
        for path in sources {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            let syntax = syn::parse_file(&source)
                .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
            for item in syntax.items {
                match item {
                    Item::Impl(item_impl)
                        if item_impl.trait_.is_none() && !is_core_type(&item_impl.self_ty) =>
                    {
                        let Some(type_name) = simple_type_name(&item_impl.self_ty) else {
                            continue;
                        };
                        for item in item_impl.items {
                            let ImplItem::Fn(method) = item else {
                                continue;
                            };
                            if !matches!(method.vis, Visibility::Public(_)) {
                                continue;
                            }
                            let has_mutable_receiver =
                                method.sig.inputs.first().is_some_and(|input| {
                                    matches!(input, FnArg::Receiver(receiver) if receiver.mutability.is_some())
                                });
                            if has_mutable_receiver || signature_has_path(&method.sig) {
                                mutations.insert(format!("{type_name}::{}", method.sig.ident));
                            }
                        }
                    }
                    Item::Fn(function)
                        if matches!(function.vis, Visibility::Public(_))
                            && signature_has_mutable_reference(&function.sig) =>
                    {
                        mutations.insert(format!("{crate_name}::{}", function.sig.ident));
                    }
                    _ => {}
                }
            }
        }
    }

    let mut format_sources = Vec::new();
    collect_rust_sources(
        &repository.join("rust/inkpod-format/src"),
        &mut format_sources,
    );
    for path in format_sources {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let syntax = syn::parse_file(&source)
            .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
        for item in syntax.items {
            let Item::Fn(function) = item else {
                continue;
            };
            let name = function.sig.ident.to_string();
            if matches!(function.vis, Visibility::Public(_))
                && (signature_has_path(&function.sig)
                    || signature_has_mutable_reference(&function.sig))
            {
                mutations.insert(format!("inkpod_format::{name}"));
            }
        }
    }
    mutations
}

fn public_ffi_exports(source_root: &Path) -> BTreeSet<String> {
    let mut sources = Vec::new();
    collect_rust_sources(source_root, &mut sources);
    let mut exports = BTreeSet::new();
    for path in sources {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let syntax = syn::parse_file(&source)
            .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
        for item in syntax.items {
            let Item::Fn(function) = item else {
                continue;
            };
            let is_c_abi = function
                .sig
                .abi
                .as_ref()
                .and_then(|abi| abi.name.as_ref())
                .is_some_and(|name| name.value() == "C");
            let name = function.sig.ident.to_string();
            if is_c_abi && matches!(function.vis, Visibility::Public(_)) {
                assert!(
                    name.starts_with("inkpod_"),
                    "public C ABI export {name} is outside the canonical inkpod_ namespace"
                );
                assert!(
                    exports.insert(name.clone()),
                    "duplicate C ABI export {name}"
                );
            }
        }
    }
    exports
}

fn windows_production_commands(resource: &str) -> BTreeSet<String> {
    let bytes = resource.as_bytes();
    let mut commands = BTreeSet::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let Some(relative) = resource[offset..].find("IDM_") else {
            break;
        };
        let start = offset + relative;
        let mut end = start + 4;
        while end < bytes.len()
            && (bytes[end].is_ascii_uppercase()
                || bytes[end].is_ascii_digit()
                || bytes[end] == b'_')
        {
            end += 1;
        }
        commands.insert(resource[start..end].to_owned());
        offset = end;
    }
    commands
}

fn parse_inventory(text: &str) -> Vec<Route<'_>> {
    let mut routes = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        let Some(record) = line.strip_prefix("route|") else {
            continue;
        };
        let fields = record.split('|').collect::<Vec<_>>();
        assert_eq!(
            fields.len(),
            4,
            "route inventory line {} must contain surface, category, owner, and symbols",
            line_index + 1
        );
        let [surface, category, owner, symbols] = fields.as_slice() else {
            unreachable!();
        };
        assert!(
            matches!(*surface, "rust" | "ffi" | "windows"),
            "unknown route surface {surface} on line {}",
            line_index + 1
        );
        assert!(
            ROUTE_CATEGORIES.contains(category),
            "unknown route category {category} on line {}",
            line_index + 1
        );
        assert!(
            ROUTE_OWNERS.contains(owner),
            "unknown route owner {owner} on line {}",
            line_index + 1
        );
        let mut symbol_count = 0;
        for symbol in symbols.split_ascii_whitespace() {
            symbol_count += 1;
            routes.push(Route {
                surface,
                category,
                owner,
                symbol,
            });
        }
        assert!(
            symbol_count > 0,
            "route inventory line {} has no symbols",
            line_index + 1
        );
    }
    assert!(
        !routes.is_empty(),
        "route inventory contains no machine-readable routes"
    );
    routes
}

#[test]
fn route_inventory_covers_public_core_ffi_and_windows_surfaces() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("Core crate must be below the repository root");
    let inventory_path = repository.join("docs/primitive-route-inventory.md");
    let inventory = fs::read_to_string(&inventory_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", inventory_path.display()));
    let routes = parse_inventory(&inventory);

    let mut inventoried = BTreeMap::<&str, BTreeSet<&str>>::new();
    let mut categories = BTreeSet::new();
    for route in &routes {
        categories.insert(route.category);
        let inserted = inventoried
            .entry(route.surface)
            .or_default()
            .insert(route.symbol);
        assert!(
            inserted,
            "{} route {} has more than one classification/owner",
            route.surface, route.symbol
        );
        if matches!(
            route.category,
            "document-primitive" | "history-control-event"
        ) {
            assert_eq!(
                route.owner, "rust-core",
                "{} route {} assigns Rust document/history semantics to {}",
                route.surface, route.symbol, route.owner
            );
        }
    }
    assert_eq!(
        categories,
        ROUTE_CATEGORIES.into_iter().collect(),
        "every route category must remain represented and spelled canonically"
    );

    let mut expected_rust = public_core_methods(&repository.join("rust/inkpod-core/src"));
    expected_rust.extend(public_non_core_mutations(repository));
    let expected_ffi = public_ffi_exports(&repository.join("rust/inkpod-ffi/src"));
    let resource_path = repository.join("apps/windows/app/app.rc");
    let resource = fs::read_to_string(&resource_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", resource_path.display()));
    let expected_windows = windows_production_commands(&resource);

    let actual_rust = inventoried
        .get("rust")
        .into_iter()
        .flat_map(|routes| routes.iter().copied().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    let actual_ffi = inventoried
        .get("ffi")
        .into_iter()
        .flat_map(|routes| routes.iter().copied().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    let actual_windows = inventoried
        .get("windows")
        .into_iter()
        .flat_map(|routes| routes.iter().copied().map(str::to_owned))
        .collect::<BTreeSet<_>>();

    assert_eq!(
        actual_rust, expected_rust,
        "public Core route inventory drifted"
    );
    assert_eq!(actual_ffi, expected_ffi, "C ABI route inventory drifted");
    assert_eq!(
        actual_windows, expected_windows,
        "Windows production command route inventory drifted"
    );
}
