use super::*;

impl Registry {
    pub(crate) fn normalize_graph_path(path: &str) -> String {
        path.trim()
            .replace('\\', "/")
            .trim_start_matches("./")
            .trim_start_matches('/')
            .to_string()
    }

    pub(crate) fn path_in_partition(path: &str, partition_path: &str) -> bool {
        let normalized_path = Self::normalize_graph_path(path);
        let normalized_partition = Self::normalize_graph_path(partition_path);
        if normalized_partition.is_empty() {
            return false;
        }
        normalized_path == normalized_partition
            || normalized_path.starts_with(&format!("{normalized_partition}/"))
    }
}
