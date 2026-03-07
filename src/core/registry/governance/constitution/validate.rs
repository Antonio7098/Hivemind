use super::*;

impl Registry {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn validate_constitution(
        artifact: &ConstitutionArtifact,
    ) -> Vec<ConstitutionValidationIssue> {
        let mut issues = Vec::new();
        if artifact.version != CONSTITUTION_VERSION {
            issues.push(ConstitutionValidationIssue {
                code: "constitution_version_unsupported".to_string(),
                field: "version".to_string(),
                message: format!(
                    "Unsupported constitution version {}; expected {CONSTITUTION_VERSION}",
                    artifact.version
                ),
            });
        }
        if artifact.schema_version.trim() != CONSTITUTION_SCHEMA_VERSION {
            issues.push(ConstitutionValidationIssue {
                code: "constitution_schema_version_unsupported".to_string(),
                field: "schema_version".to_string(),
                message: format!(
                    "Unsupported schema_version '{}'; expected '{CONSTITUTION_SCHEMA_VERSION}'",
                    artifact.schema_version
                ),
            });
        }
        if artifact
            .compatibility
            .minimum_hivemind_version
            .trim()
            .is_empty()
        {
            issues.push(ConstitutionValidationIssue {
                code: "constitution_minimum_hivemind_version_missing".to_string(),
                field: "compatibility.minimum_hivemind_version".to_string(),
                message: "minimum_hivemind_version cannot be empty".to_string(),
            });
        }
        if artifact.compatibility.governance_schema_version.trim() != GOVERNANCE_SCHEMA_VERSION {
            issues.push(ConstitutionValidationIssue {
                code: "constitution_governance_schema_mismatch".to_string(),
                field: "compatibility.governance_schema_version".to_string(),
                message: format!("governance_schema_version must be '{GOVERNANCE_SCHEMA_VERSION}'"),
            });
        }

        let mut partition_ids = HashSet::new();
        let mut partition_paths = HashSet::new();
        for partition in &artifact.partitions {
            let partition_id = partition.id.trim();
            if partition_id.is_empty() {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_partition_id_missing".to_string(),
                    field: "partitions[].id".to_string(),
                    message: "Partition id cannot be empty".to_string(),
                });
            } else if !partition_ids.insert(partition_id.to_string()) {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_partition_id_duplicate".to_string(),
                    field: format!("partitions.{}", partition.id),
                    message: format!("Duplicate partition id '{}'", partition.id),
                });
            }
            let partition_path = partition.path.trim();
            if partition_path.is_empty() {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_partition_path_missing".to_string(),
                    field: format!("partitions.{}.path", partition.id),
                    message: "Partition path cannot be empty".to_string(),
                });
            } else if !partition_paths.insert(partition_path.to_string()) {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_partition_path_duplicate".to_string(),
                    field: format!("partitions.{}.path", partition.id),
                    message: format!("Duplicate partition path '{}'", partition.path),
                });
            }
        }

        let mut rule_ids = HashSet::new();
        for rule in &artifact.rules {
            let rule_id = rule.id().trim();
            if rule_id.is_empty() {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_rule_id_missing".to_string(),
                    field: "rules[].id".to_string(),
                    message: "Rule id cannot be empty".to_string(),
                });
                continue;
            }
            if !rule_ids.insert(rule_id.to_string()) {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_rule_id_duplicate".to_string(),
                    field: format!("rules.{rule_id}"),
                    message: format!("Duplicate rule id '{rule_id}'"),
                });
            }
            match rule {
                ConstitutionRule::ForbiddenDependency { from, to, .. }
                | ConstitutionRule::AllowedDependency { from, to, .. } => {
                    if from.trim() == to.trim() {
                        issues.push(ConstitutionValidationIssue {
                            code: "constitution_rule_self_dependency".to_string(),
                            field: format!("rules.{rule_id}"),
                            message: "Rule cannot reference the same partition for from/to"
                                .to_string(),
                        });
                    }
                    for (field, partition_ref) in [("from", from), ("to", to)] {
                        if partition_ref.trim().is_empty() {
                            issues.push(ConstitutionValidationIssue {
                                code: "constitution_rule_partition_missing".to_string(),
                                field: format!("rules.{rule_id}.{field}"),
                                message: format!("Rule field '{field}' cannot be empty"),
                            });
                        } else if !partition_ids.contains(partition_ref.trim()) {
                            issues.push(ConstitutionValidationIssue {
                                code: "constitution_rule_partition_unknown".to_string(),
                                field: format!("rules.{rule_id}.{field}"),
                                message: format!(
                                    "Rule references unknown partition '{partition_ref}'"
                                ),
                            });
                        }
                    }
                }
                ConstitutionRule::CoverageRequirement {
                    target, threshold, ..
                } => {
                    if target.trim().is_empty() {
                        issues.push(ConstitutionValidationIssue {
                            code: "constitution_rule_target_missing".to_string(),
                            field: format!("rules.{rule_id}.target"),
                            message: "Coverage target partition cannot be empty".to_string(),
                        });
                    } else if !partition_ids.contains(target.trim()) {
                        issues.push(ConstitutionValidationIssue {
                            code: "constitution_rule_target_unknown".to_string(),
                            field: format!("rules.{rule_id}.target"),
                            message: format!(
                                "Coverage rule references unknown partition '{target}'"
                            ),
                        });
                    }
                    if *threshold > 100 {
                        issues.push(ConstitutionValidationIssue {
                            code: "constitution_rule_threshold_invalid".to_string(),
                            field: format!("rules.{rule_id}.threshold"),
                            message: "Coverage threshold must be between 0 and 100".to_string(),
                        });
                    }
                }
            }
        }
        issues
    }
}
