use cargo_goose::toolchain::{check_runtime, parse_goose_info};

#[test]
fn stock_compilers_have_no_optional_capabilities() {
    let info = parse_goose_info("goose runtime a5dbd11b6309\n").unwrap();
    assert!(info.features.is_empty());
    assert_eq!(info.native_abi, None);
    check_runtime(&info, "a5dbd11b6309", "test library", false).unwrap();
}

#[test]
fn capabilities_and_native_abi_are_independent() {
    let info = parse_goose_info(
        "goose runtime 0123456789ab\nfeatures: typed-builders emit-interface native-build\nnative-abi: 1\n",
    ).unwrap();
    assert!(info.has("typed-builders"));
    assert!(info.has("native-build"));
    check_runtime(&info, "a5dbd11b6309", "test library", false).unwrap();

    let unknown = parse_goose_info("goose runtime 0123456789ab\nfeatures: typed-builders\n").unwrap();
    assert!(check_runtime(&unknown, "a5dbd11b6309", "test library", false).is_err());
    let incompatible = parse_goose_info("goose runtime a5dbd11b6309\nnative-abi: 99\n").unwrap();
    assert!(check_runtime(&incompatible, "a5dbd11b6309", "test library", false).is_err());
    assert!(check_runtime(&incompatible, "a5dbd11b6309", "test library", true).is_ok());
}

#[test]
fn malformed_version_reports_fail() {
    assert!(parse_goose_info("features: typed-builders\n").is_err());
    assert!(parse_goose_info("goose runtime abc\nnative-abi: unknown\n").is_err());
}
