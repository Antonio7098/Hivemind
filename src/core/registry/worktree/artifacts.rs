use super::*;

impl Registry {
    pub(crate) fn artifacts_dir(&self) -> PathBuf {
        self.config.data_dir.join("artifacts")
    }

    pub(crate) fn baselines_dir(&self) -> PathBuf {
        self.artifacts_dir().join("baselines")
    }

    pub(crate) fn baseline_dir(&self, baseline_id: Uuid) -> PathBuf {
        self.baselines_dir().join(baseline_id.to_string())
    }

    pub(crate) fn baseline_json_path(&self, baseline_id: Uuid) -> PathBuf {
        self.baseline_dir(baseline_id).join("baseline.json")
    }

    pub(crate) fn baseline_files_dir(&self, baseline_id: Uuid) -> PathBuf {
        self.baseline_dir(baseline_id).join("files")
    }

    pub(crate) fn diffs_dir(&self) -> PathBuf {
        self.artifacts_dir().join("diffs")
    }

    pub(crate) fn diff_json_path(&self, diff_id: Uuid) -> PathBuf {
        self.diffs_dir().join(format!("{diff_id}.json"))
    }

    pub(crate) fn scope_traces_dir(&self) -> PathBuf {
        self.artifacts_dir().join("scope-traces")
    }

    pub(crate) fn scope_trace_path(&self, attempt_id: Uuid) -> PathBuf {
        self.scope_traces_dir().join(format!("{attempt_id}.log"))
    }

    pub(crate) fn scope_baselines_dir(&self) -> PathBuf {
        self.artifacts_dir().join("scope-baselines")
    }

    pub(crate) fn scope_baseline_path(&self, attempt_id: Uuid) -> PathBuf {
        self.scope_baselines_dir()
            .join(format!("{attempt_id}.json"))
    }
}
