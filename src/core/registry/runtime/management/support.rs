use super::*;
mod health;
mod selection;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_runtime_env_pairs_accepts_key_value_entries() {
        let parsed = Registry::parse_runtime_env_pairs(
            &["FOO=bar".to_string(), "BAZ=qux".to_string()],
            "runtime:test",
        )
        .expect("valid env pairs");

        assert_eq!(parsed.get("FOO").map(String::as_str), Some("bar"));
        assert_eq!(parsed.get("BAZ").map(String::as_str), Some("qux"));
    }

    #[test]
    fn parse_global_parallel_limit_rejects_zero() {
        let err = Registry::parse_global_parallel_limit(Some("0".to_string())).unwrap_err();
        assert!(err.code.contains("invalid_global_parallel_limit"));
    }

    #[test]
    fn ensure_supported_runtime_adapter_accepts_native() {
        Registry::ensure_supported_runtime_adapter("native", "runtime:test")
            .expect("native adapter should be supported");
    }

    #[test]
    fn supported_runtime_descriptors_include_native() {
        let descriptors = Registry::supported_runtime_descriptors();
        assert!(descriptors.iter().any(|descriptor| {
            descriptor.adapter_name == "native"
                && descriptor.default_binary == "builtin-native"
                && !descriptor.requires_binary
        }));
    }
}
