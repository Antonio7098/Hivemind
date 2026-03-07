use super::*;

impl Registry {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn evaluate_constitution_rules(
        artifact: &ConstitutionArtifact,
        snapshot: &GraphSnapshotArtifact,
        origin: &'static str,
    ) -> Result<Vec<ConstitutionRuleViolation>> {
        let facts = Self::graph_facts_from_snapshot(snapshot, origin)?;
        let mut partition_files: HashMap<String, HashSet<String>> = HashMap::new();
        for partition in &artifact.partitions {
            let mut members = HashSet::new();
            for (graph_key, file) in &facts.files {
                if Self::path_in_partition(&file.path, &partition.path) {
                    members.insert(graph_key.clone());
                }
            }
            partition_files.insert(partition.id.clone(), members);
        }

        let mut violations = Vec::new();
        for rule in &artifact.rules {
            match rule {
                ConstitutionRule::ForbiddenDependency {
                    id,
                    from,
                    to,
                    severity,
                } => {
                    let from_files = partition_files.get(from).cloned().unwrap_or_default();
                    let to_files = partition_files.get(to).cloned().unwrap_or_default();
                    let mut evidence = Vec::new();
                    for source_key in &from_files {
                        let Some(source) = facts.files.get(source_key) else {
                            continue;
                        };
                        for target_key in &source.references {
                            if !to_files.contains(target_key) {
                                continue;
                            }
                            let target = facts
                                .files
                                .get(target_key)
                                .map_or(target_key.as_str(), |item| item.display_path.as_str());
                            evidence.push(format!("{} -> {target}", source.display_path));
                        }
                    }
                    if !evidence.is_empty() {
                        violations.push(ConstitutionRuleViolation {
                            rule_id: id.clone(),
                            rule_type: "forbidden_dependency".to_string(),
                            severity: severity.clone(),
                            message: format!("Detected {} forbidden dependency edge(s) from partition '{from}' to '{to}'", evidence.len()),
                            evidence: evidence.into_iter().take(20).collect(),
                            remediation_hint: Some(format!("Remove or invert dependencies from '{from}' to '{to}'")),
                            blocked: matches!(severity, ConstitutionSeverity::Hard),
                        });
                    }
                }
                ConstitutionRule::AllowedDependency {
                    id,
                    from,
                    to,
                    severity,
                } => {
                    let from_files = partition_files.get(from).cloned().unwrap_or_default();
                    let to_files = partition_files.get(to).cloned().unwrap_or_default();
                    let mut evidence = Vec::new();
                    for source_key in &from_files {
                        let Some(source) = facts.files.get(source_key) else {
                            continue;
                        };
                        for target_key in &source.references {
                            if from_files.contains(target_key) || to_files.contains(target_key) {
                                continue;
                            }
                            let violating_partitions: Vec<&str> = partition_files
                                .iter()
                                .filter_map(|(partition_id, members)| {
                                    members
                                        .contains(target_key)
                                        .then_some(partition_id.as_str())
                                })
                                .collect();
                            if violating_partitions.is_empty() {
                                continue;
                            }
                            let target = facts
                                .files
                                .get(target_key)
                                .map_or(target_key.as_str(), |item| item.display_path.as_str());
                            evidence.push(format!(
                                "{} -> {target} (target partitions: {})",
                                source.display_path,
                                violating_partitions.join(", ")
                            ));
                        }
                    }
                    if !evidence.is_empty() {
                        violations.push(ConstitutionRuleViolation {
                            rule_id: id.clone(),
                            rule_type: "allowed_dependency".to_string(),
                            severity: severity.clone(),
                            message: format!("Detected {} dependency edge(s) from partition '{from}' outside allowed target partition '{to}'", evidence.len()),
                            evidence: evidence.into_iter().take(20).collect(),
                            remediation_hint: Some(format!("Restrict '{from}' dependencies to partition '{to}' (and intra-partition references)")),
                            blocked: matches!(severity, ConstitutionSeverity::Hard),
                        });
                    }
                }
                ConstitutionRule::CoverageRequirement {
                    id,
                    target,
                    threshold,
                    severity,
                } => {
                    let target_files = partition_files.get(target).cloned().unwrap_or_default();
                    let total = target_files.len();
                    let covered = target_files
                        .iter()
                        .filter(|file_key| {
                            facts.files.get(*file_key).is_some_and(|file| {
                                facts.symbol_file_keys.contains(&file.symbol_key)
                            })
                        })
                        .count();
                    let coverage_bps: u128 = if total == 0 {
                        0
                    } else {
                        (covered as u128 * 10_000) / total as u128
                    };
                    let coverage_whole = coverage_bps / 100;
                    let coverage_frac = coverage_bps % 100;
                    let meets_threshold =
                        (covered as u128 * 100) >= (u128::from(*threshold) * total as u128);
                    if !meets_threshold {
                        let missing_files: Vec<String> = target_files
                            .iter()
                            .filter_map(|file_key| {
                                facts.files.get(file_key).and_then(|file| {
                                    (!facts.symbol_file_keys.contains(&file.symbol_key))
                                        .then(|| file.display_path.clone())
                                })
                            })
                            .take(10)
                            .collect();
                        let mut evidence = vec![format!("coverage={coverage_whole}.{coverage_frac:02}% threshold={threshold}% covered_files={covered}/{total}")];
                        if !missing_files.is_empty() {
                            evidence.push(format!(
                                "files without symbol coverage: {}",
                                missing_files.join(", ")
                            ));
                        }
                        violations.push(ConstitutionRuleViolation {
                            rule_id: id.clone(),
                            rule_type: "coverage_requirement".to_string(),
                            severity: severity.clone(),
                            message: format!("Partition '{target}' coverage is below threshold ({coverage_whole}.{coverage_frac:02}% < {threshold}%)"),
                            evidence,
                            remediation_hint: Some(format!("Increase codegraph symbol coverage for files under '{target}' or lower the threshold")),
                            blocked: matches!(severity, ConstitutionSeverity::Hard),
                        });
                    }
                }
            }
        }

        Ok(violations)
    }
}
