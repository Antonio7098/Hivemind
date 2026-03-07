use super::parser::{is_narrative_line, parse_command, parse_todo_item, parse_tool_name};

pub(super) fn starts_new_projection_line(line: &str) -> bool {
    parse_command(line).is_some()
        || parse_tool_name(line).is_some()
        || parse_todo_item(line).is_some()
        || is_narrative_line(line)
}

pub(super) fn normalize_projection_line(raw: &str) -> String {
    let no_ansi = strip_ansi_sequences(raw);
    no_ansi
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\r' || *c == '\t')
        .collect::<String>()
}

fn strip_ansi_sequences(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek().is_some_and(|c| *c == '[') {
                let _ = chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
                continue;
            }
            continue;
        }
        out.push(ch);
    }

    out
}
