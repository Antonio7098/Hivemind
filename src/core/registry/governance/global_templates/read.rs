use super::*;

impl Registry {
    pub(crate) fn global_template_path(&self, template_id: &str) -> PathBuf {
        self.governance_global_root()
            .join("templates")
            .join(format!("{template_id}.json"))
    }

    pub(crate) fn read_global_template_artifact(
        &self,
        template_id: &str,
        origin: &'static str,
    ) -> Result<GlobalTemplateArtifact> {
        let path = self.global_template_path(template_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "template_not_found",
                format!("Global template '{template_id}' not found"),
                origin,
            )
            .with_hint("Use 'hivemind global template list' to inspect available templates"));
        }
        let artifact = Self::read_governance_json::<GlobalTemplateArtifact>(
            &path,
            "global_template",
            template_id,
            origin,
        )?;
        if artifact.template_id != template_id {
            return Err(HivemindError::user(
                "governance_artifact_schema_invalid",
                format!(
                    "Template file key mismatch: expected '{template_id}', found '{}'",
                    artifact.template_id
                ),
                origin,
            )
            .with_context("path", path.to_string_lossy().to_string()));
        }
        Ok(artifact)
    }
}
