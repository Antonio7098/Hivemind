use super::*;

impl NativeAdapterConfig {
    #[must_use]
    pub fn new() -> Self {
        Self {
            base: AdapterConfig::new("native", PathBuf::from("builtin-native"))
                .with_timeout(Duration::from_secs(300)),
            native: NativeRuntimeConfig::default(),
            scripted_directives: vec![
                "ACT:tool:list_files:{\"path\":\".\",\"recursive\":false}".to_string(),
                "DONE:native runtime completed deterministically".to_string(),
            ],
            provider_name: "mock".to_string(),
            model_name: "native-mock-v1".to_string(),
        }
    }
}

impl Default for NativeAdapterConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeRuntimeAdapter {
    pub(crate) fn capture_mode(config: &NativeRuntimeConfig) -> NativePayloadCaptureMode {
        if config.capture_full_payloads {
            NativePayloadCaptureMode::FullPayload
        } else {
            NativePayloadCaptureMode::MetadataOnly
        }
    }
}
