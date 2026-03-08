pub(super) fn parse_command(line: &str) -> Option<String> {
    if is_raw_provider_json_mirror_line(line) {
        return None;
    }
    if let Some(rest) = line.find("Command: ").map(|idx| &line[idx..]) {
        let cmd = rest.trim_start_matches("Command: ").trim();
        if !cmd.is_empty() {
            return Some(cmd.to_string());
        }
    }
    if let Some(rest) = line.find("Running command: ").map(|idx| &line[idx..]) {
        let cmd = rest.trim_start_matches("Running command: ").trim();
        if !cmd.is_empty() {
            return Some(cmd.to_string());
        }
    }

    for prefix in [
        "$ ",
        "> ",
        "Command: ",
        "Running: ",
        "Running command: ",
        "Executing: ",
        "Execute: ",
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            let cmd = rest.trim();
            if !cmd.is_empty() {
                return Some(cmd.to_string());
            }
        }
    }
    None
}

pub(super) fn parse_tool_name(line: &str) -> Option<String> {
    if is_raw_provider_json_mirror_line(line) {
        return None;
    }
    if let Some(rest) = line.find("Tool: ").map(|idx| &line[idx..]) {
        let name = rest
            .trim_start_matches("Tool: ")
            .split_whitespace()
            .next()?;
        return Some(name.to_string());
    }
    if let Some(rest) = line.strip_prefix("Tool: ") {
        let name = rest.split_whitespace().next()?;
        return Some(name.to_string());
    }
    if let Some(rest) = line.strip_prefix("Using tool ") {
        let name = rest
            .split(|c: char| c.is_whitespace() || c == ':' || c == '(')
            .next()?;
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    if let Some(rest) = line.strip_prefix("tool=") {
        let name = rest
            .split(|c: char| c.is_whitespace() || c == ',' || c == ')')
            .next()?;
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

pub(super) fn parse_todo_item(line: &str) -> Option<(String, bool)> {
    if is_raw_provider_json_mirror_line(line) {
        return None;
    }
    for (prefix, completed) in [
        ("- [ ] ", false),
        ("* [ ] ", false),
        ("- [x] ", true),
        ("* [x] ", true),
        ("- [X] ", true),
        ("* [X] ", true),
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            let item = rest.trim();
            if !item.is_empty() {
                return Some((item.to_string(), completed));
            }
        }
    }
    for (prefix, completed) in [
        ("TODO: ", false),
        ("TODO ", false),
        ("DONE: ", true),
        ("DONE ", true),
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            let item = rest.trim();
            if !item.is_empty() {
                return Some((item.to_string(), completed));
            }
        }
    }
    None
}

pub(super) fn is_narrative_line(line: &str) -> bool {
    if is_raw_provider_json_mirror_line(line) {
        return false;
    }
    let lower = line.to_lowercase();
    lower.starts_with("i ")
        || lower.starts_with("i'")
        || lower.starts_with("i\"")
        || lower.starts_with("task:")
        || lower.starts_with("next ")
        || lower.starts_with("plan:")
        || lower.starts_with("because")
        || lower.starts_with("thinking:")
        || lower.starts_with("hello")
        || lower.starts_with("starting")
        || lower.starts_with("working")
        || lower.starts_with("updating")
        || lower.starts_with("checking")
        || lower.starts_with("analyzing")
        || lower.starts_with("investigating")
}

fn is_raw_provider_json_mirror_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('[') {
        return false;
    }
    let Some(end_bracket) = trimmed.find(']') else {
        return false;
    };
    let label = &trimmed[1..end_bracket];
    std::path::Path::new(label)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
}
