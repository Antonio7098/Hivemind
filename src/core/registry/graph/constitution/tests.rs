use super::*;

#[test]
fn normalize_graph_path_trims_and_normalizes_separators() {
    assert_eq!(
        Registry::normalize_graph_path(" ./src\\core/file.rs "),
        "src/core/file.rs"
    );
}

#[test]
fn path_in_partition_matches_directories_only() {
    assert!(Registry::path_in_partition("src/core/file.rs", "src/core"));
    assert!(Registry::path_in_partition("src/core", "src/core"));
    assert!(!Registry::path_in_partition(
        "src/core-utils/file.rs",
        "src/core"
    ));
    assert!(!Registry::path_in_partition("src/core/file.rs", ""));
}
