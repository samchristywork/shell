use glob::glob;
use std::env;

pub fn expand_tilde(path: &str) -> String {
    if path == "~" {
        dirs::home_dir()
            .map(|home| home.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string())
    } else if path.starts_with("~/") {
        dirs::home_dir()
            .map(|home| home.join(&path[2..]).to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string())
    } else {
        path.to_string()
    }
}

pub fn expand_variables(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            if let Some(&next_char) = chars.peek() {
                if next_char == '{' {
                    chars.next();
                    let mut var_name = String::new();
                    let mut found_closing = false;

                    for c in chars.by_ref() {
                        if c == '}' {
                            found_closing = true;
                            break;
                        }
                        var_name.push(c);
                    }

                    if found_closing {
                        if let Ok(value) = env::var(&var_name) {
                            result.push_str(&value);
                        }
                    } else {
                        result.push_str("${");
                        result.push_str(&var_name);
                    }
                } else if next_char.is_alphabetic() || next_char == '_' {
                    let mut var_name = String::new();

                    while let Some(&next_char) = chars.peek() {
                        if next_char.is_alphanumeric() || next_char == '_' {
                            var_name.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }

                    if let Ok(value) = env::var(&var_name) {
                        result.push_str(&value);
                    }
                } else {
                    result.push(c);
                }
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    result
}

pub fn expand_globs(arg: &str) -> Vec<String> {
    if arg.contains('*') || arg.contains('?') || arg.contains('[') {
        match glob(arg) {
            Ok(paths) => {
                let mut matches: Vec<String> = paths
                    .filter_map(|path| path.ok())
                    .map(|path| path.to_string_lossy().to_string())
                    .collect();

                matches.sort();

                if matches.is_empty() {
                    vec![arg.to_string()]
                } else {
                    matches
                }
            }
            Err(_) => {
                vec![arg.to_string()]
            }
        }
    } else {
        vec![arg.to_string()]
    }
}

pub fn parse_arguments(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current_arg = String::new();
    let mut in_quotes = false;
    let mut quote_char = '"';
    let mut was_quoted = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = c;
                was_quoted = true;
            }
            c if in_quotes && c == quote_char => {
                in_quotes = false;
            }
            ' ' | '\t' if !in_quotes => {
                if !current_arg.is_empty() {
                    let expanded = expand_variables(&current_arg);
                    let tilde_expanded = expand_tilde(&expanded);

                    if was_quoted {
                        args.push(tilde_expanded);
                    } else {
                        let glob_expanded = expand_globs(&tilde_expanded);
                        args.extend(glob_expanded);
                    }

                    current_arg.clear();
                    was_quoted = false;
                }

                while let Some(&next_char) = chars.peek() {
                    if next_char == ' ' || next_char == '\t' {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            _ => {
                current_arg.push(c);
            }
        }
    }

    if !current_arg.is_empty() {
        let expanded = expand_variables(&current_arg);
        let tilde_expanded = expand_tilde(&expanded);

        if was_quoted {
            args.push(tilde_expanded);
        } else {
            let glob_expanded = expand_globs(&tilde_expanded);
            args.extend(glob_expanded);
        }
    }

    args
}

pub fn split_commands(input: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut current_command = String::new();
    let mut in_quotes = false;
    let mut quote_char = '"';
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = c;
                current_command.push(c);
            }
            c if in_quotes && c == quote_char => {
                in_quotes = false;
                current_command.push(c);
            }
            ';' if !in_quotes => {
                if !current_command.trim().is_empty() {
                    commands.push(current_command.trim().to_string());
                }
                current_command.clear();
            }
            _ => {
                current_command.push(c);
            }
        }
    }

    if !current_command.trim().is_empty() {
        commands.push(current_command.trim().to_string());
    }

    commands
}
