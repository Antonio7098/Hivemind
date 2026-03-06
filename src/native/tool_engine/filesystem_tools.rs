use super::*;

fn ensure_can_read(scope: Option<&Scope>, rel_path: &Path) -> Result<(), NativeToolEngineError> {
    if let Some(scope) = scope {
        let rel = relative_display(rel_path);
        if !scope.filesystem.can_read(&rel) {
            return Err(NativeToolEngineError::scope_violation(format!(
                "read is outside scope: {rel}"
            )));
        }
    }
    Ok(())
}

fn ensure_can_write(scope: Option<&Scope>, rel_path: &Path) -> Result<(), NativeToolEngineError> {
    if let Some(scope) = scope {
        let rel = relative_display(rel_path);
        if !scope.filesystem.can_write(&rel) {
            return Err(NativeToolEngineError::scope_violation(format!(
                "write is outside scope: {rel}"
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadFileInput {
    path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadFileOutput {
    path: String,
    content: String,
    byte_size: u64,
}

pub(super) fn handle_read_file(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<ReadFileInput>(input)?;
    let rel = normalize_relative_path(&input.path, false)?;
    ensure_can_read(ctx.scope, &rel)?;
    let path = ctx.worktree.join(&rel);
    let content = fs::read_to_string(&path).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to read '{}': {error}", path.display()))
    })?;
    let output = ReadFileOutput {
        path: relative_display(&rel),
        byte_size: u64::try_from(content.len()).unwrap_or(u64::MAX),
        content,
    };
    serde_json::to_value(output).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode read_file output: {error}"))
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ListFilesInput {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    recursive: bool,
    #[serde(default)]
    include_hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListFilesEntry {
    path: String,
    is_dir: bool,
    byte_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ListFilesOutput {
    base_path: String,
    entries: Vec<ListFilesEntry>,
}

pub(super) fn handle_list_files(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<ListFilesInput>(input)?;
    let rel = normalize_relative_path(
        input.path.as_deref().unwrap_or("."),
        true, // allow listing current directory.
    )?;

    if !rel.as_os_str().is_empty() {
        ensure_can_read(ctx.scope, &rel)?;
    }
    let root = ctx.worktree.join(&rel);
    let mut entries = Vec::new();
    list_entries(
        ctx,
        &root,
        &rel,
        input.recursive,
        input.include_hidden,
        &mut entries,
    )?;

    let output = ListFilesOutput {
        base_path: relative_display(&rel),
        entries,
    };
    serde_json::to_value(output).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode list_files output: {error}"))
    })
}

fn list_entries(
    ctx: &ToolExecutionContext<'_>,
    absolute_dir: &Path,
    relative_dir: &Path,
    recursive: bool,
    include_hidden: bool,
    entries: &mut Vec<ListFilesEntry>,
) -> Result<(), NativeToolEngineError> {
    let read_dir = fs::read_dir(absolute_dir).map_err(|error| {
        NativeToolEngineError::execution(format!(
            "failed to read directory '{}': {error}",
            absolute_dir.display()
        ))
    })?;

    for item in read_dir {
        let item = item.map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to inspect directory entry in '{}': {error}",
                absolute_dir.display()
            ))
        })?;
        let file_name = item.file_name();
        let name = file_name.to_string_lossy();
        if !include_hidden && name.starts_with('.') {
            continue;
        }

        let rel_child = if relative_dir.as_os_str().is_empty() {
            PathBuf::from(file_name)
        } else {
            relative_dir.join(file_name)
        };
        if let Some(scope) = ctx.scope {
            let rel_str = relative_display(&rel_child);
            if !scope.filesystem.can_read(&rel_str) {
                continue;
            }
        }

        let metadata = item.metadata().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to read metadata for '{}': {error}",
                item.path().display()
            ))
        })?;
        let is_dir = metadata.is_dir();
        let byte_size = if is_dir { 0 } else { metadata.len() };
        entries.push(ListFilesEntry {
            path: relative_display(&rel_child),
            is_dir,
            byte_size,
        });

        if recursive && is_dir {
            list_entries(ctx, &item.path(), &rel_child, true, include_hidden, entries)?;
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct WriteFileInput {
    pub(super) path: String,
    content: String,
    #[serde(default)]
    append: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct WriteFileOutput {
    path: String,
    bytes_written: u64,
    append: bool,
}

pub(super) fn handle_write_file(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<WriteFileInput>(input)?;
    let rel = normalize_relative_path(&input.path, false)?;
    ensure_can_write(ctx.scope, &rel)?;

    let absolute = ctx.worktree.join(&rel);
    if let Some(parent) = absolute.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to create parent directory '{}': {error}",
                parent.display()
            ))
        })?;
    }

    let mut options = fs::OpenOptions::new();
    options.create(true).write(true);
    if input.append {
        options.append(true);
    } else {
        options.truncate(true);
    }

    let mut file = options.open(&absolute).map_err(|error| {
        NativeToolEngineError::execution(format!(
            "failed to open '{}': {error}",
            absolute.display()
        ))
    })?;
    file.write_all(input.content.as_bytes()).map_err(|error| {
        NativeToolEngineError::execution(format!(
            "failed to write '{}': {error}",
            absolute.display()
        ))
    })?;

    let output = WriteFileOutput {
        path: relative_display(&rel),
        bytes_written: u64::try_from(input.content.len()).unwrap_or(u64::MAX),
        append: input.append,
    };
    serde_json::to_value(output).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode write_file output: {error}"))
    })
}
