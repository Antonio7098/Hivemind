pub(super) fn dangerous_command_reason(command: &str, args: &[String]) -> Option<String> {
    let command = command.trim().to_ascii_lowercase();
    let args_joined = args
        .iter()
        .map(|arg| arg.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    let has_arg = |target: &str| args.iter().any(|arg| arg.eq_ignore_ascii_case(target));
    if command == "rm"
        && (has_arg("-rf") || has_arg("-fr") || has_arg("-r") && has_arg("-f"))
        && args
            .iter()
            .any(|arg| arg == "/" || arg == "~" || arg == "/*" || arg == "~/*")
    {
        return Some("rm recursive delete targeting root/home".to_string());
    }
    if matches!(
        command.as_str(),
        "sudo" | "dd" | "mkfs" | "shutdown" | "reboot"
    ) {
        return Some(format!("high-risk command '{command}'"));
    }
    if matches!(command.as_str(), "chmod" | "chown") && (has_arg("-r") || has_arg("-R")) {
        return Some(format!("recursive permission mutation via '{command}'"));
    }
    if matches!(command.as_str(), "del" | "erase")
        && (has_arg("/s") || has_arg("/f") || has_arg("/q"))
    {
        return Some("windows recursive delete flags detected".to_string());
    }
    if matches!(command.as_str(), "rmdir" | "rd") && has_arg("/s") && has_arg("/q") {
        return Some("windows recursive directory removal detected".to_string());
    }
    if command == "format" {
        return Some("disk format command detected".to_string());
    }
    if matches!(command.as_str(), "powershell" | "pwsh")
        && args_joined.contains("remove-item")
        && args_joined.contains("-recurse")
        && args_joined.contains("-force")
    {
        return Some("powershell recursive forced delete detected".to_string());
    }
    if matches!(command.as_str(), "cmd" | "cmd.exe")
        && (args_joined.contains("del /f /s /q") || args_joined.contains("rmdir /s /q"))
    {
        return Some("cmd destructive recursive delete detected".to_string());
    }
    None
}
