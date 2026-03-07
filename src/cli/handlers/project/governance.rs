use super::*;

#[allow(clippy::too_many_lines)]
pub(super) fn handle_project_governance(
    registry: &Registry,
    cmd: ProjectGovernanceCommands,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        ProjectGovernanceCommands::Init(args) => {
            let project = match support::resolve_required_selector(
                args.project.as_deref(),
                args.project_flag.as_deref(),
                "--project",
                "project",
                "cli:project:governance:init",
            ) {
                Ok(project) => project,
                Err(e) => return output_error(&e, format),
            };
            match registry.project_governance_init(&project) {
                Ok(result) => {
                    render::print_project_governance_init(&result, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceCommands::Migrate(args) => {
            match registry.project_governance_migrate(&args.project) {
                Ok(result) => {
                    render::print_project_governance_migrate(&result, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceCommands::Inspect(args) => {
            match registry.project_governance_inspect(&args.project) {
                Ok(result) => {
                    render::print_project_governance_inspect(&result, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceCommands::Diagnose(args) => {
            match registry.project_governance_diagnose(&args.project) {
                Ok(result) => {
                    print_structured(&result, format, "project governance diagnostics");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceCommands::Replay(args) => {
            match registry.project_governance_replay(&args.project, args.verify) {
                Ok(result) => {
                    print_structured(&result, format, "project governance replay result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceCommands::Snapshot(cmd) => match cmd {
            ProjectGovernanceSnapshotCommands::Create(args) => {
                match registry
                    .project_governance_snapshot_create(&args.project, args.interval_minutes)
                {
                    Ok(result) => {
                        print_structured(&result, format, "governance snapshot create result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceSnapshotCommands::List(args) => {
                match registry.project_governance_snapshot_list(&args.project, args.limit) {
                    Ok(result) => {
                        print_structured(&result, format, "governance snapshot list result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceSnapshotCommands::Restore(args) => {
                match registry.project_governance_snapshot_restore(
                    &args.project,
                    &args.snapshot_id,
                    args.confirm,
                ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance snapshot restore result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
        },
        ProjectGovernanceCommands::Repair(cmd) => match cmd {
            ProjectGovernanceRepairCommands::Detect(args) => {
                match registry.project_governance_repair_detect(&args.project) {
                    Ok(result) => {
                        print_structured(&result, format, "governance repair detect result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceRepairCommands::Preview(args) => {
                match registry
                    .project_governance_repair_preview(&args.project, args.snapshot_id.as_deref())
                {
                    Ok(result) => {
                        print_structured(&result, format, "governance repair preview result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceRepairCommands::Apply(args) => {
                match registry.project_governance_repair_apply(
                    &args.project,
                    args.snapshot_id.as_deref(),
                    args.confirm,
                ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance repair apply result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
        },
        ProjectGovernanceCommands::Document(cmd) => match cmd {
            ProjectGovernanceDocumentCommands::Create(args) => {
                match registry.project_governance_document_create(
                    &args.project,
                    &args.document_id,
                    &args.title,
                    &args.owner,
                    &args.tags,
                    &args.content,
                ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance document create result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceDocumentCommands::List(args) => {
                match registry.project_governance_document_list(&args.project) {
                    Ok(result) => {
                        print_structured(&result, format, "governance document list");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceDocumentCommands::Inspect(args) => {
                match registry.project_governance_document_inspect(&args.project, &args.document_id)
                {
                    Ok(result) => {
                        print_structured(&result, format, "governance document inspect result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceDocumentCommands::Update(args) => {
                match registry.project_governance_document_update(
                    &args.project,
                    &args.document_id,
                    args.title.as_deref(),
                    args.owner.as_deref(),
                    args.tags.as_deref(),
                    args.content.as_deref(),
                ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance document update result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceDocumentCommands::Delete(args) => {
                match registry.project_governance_document_delete(&args.project, &args.document_id)
                {
                    Ok(result) => {
                        print_structured(&result, format, "governance document delete result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
        },
        ProjectGovernanceCommands::Attachment(cmd) => match cmd {
            ProjectGovernanceAttachmentCommands::Include(args) => {
                match registry.project_governance_attachment_set_document(
                    &args.project,
                    &args.task_id,
                    &args.document_id,
                    true,
                ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance attachment include result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceAttachmentCommands::Exclude(args) => {
                match registry.project_governance_attachment_set_document(
                    &args.project,
                    &args.task_id,
                    &args.document_id,
                    false,
                ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance attachment exclude result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
        },
        ProjectGovernanceCommands::Notepad(cmd) => match cmd {
            ProjectGovernanceNotepadCommands::Create(args) => {
                match registry.project_governance_notepad_create(&args.project, &args.content) {
                    Ok(result) => {
                        print_structured(&result, format, "project notepad create result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceNotepadCommands::Show(args) => {
                match registry.project_governance_notepad_show(&args.project) {
                    Ok(result) => {
                        print_structured(&result, format, "project notepad show result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceNotepadCommands::Update(args) => {
                match registry.project_governance_notepad_update(&args.project, &args.content) {
                    Ok(result) => {
                        print_structured(&result, format, "project notepad update result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceNotepadCommands::Delete(args) => {
                match registry.project_governance_notepad_delete(&args.project) {
                    Ok(result) => {
                        print_structured(&result, format, "project notepad delete result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
        },
    }
}
