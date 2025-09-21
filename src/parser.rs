use glob::glob;
use std::collections::HashMap;

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

pub fn parse_arguments(input: &str, env_map: &HashMap<String, String>) -> Vec<String> {
    let mut args = Vec::new();
    let mut current_arg = String::new();
    let mut in_quotes = false;
    let mut quote_char = '"';
    let mut was_quoted = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' if !in_quotes || (in_quotes && quote_char == '"') => {
                if let Some(next_c) = chars.next() {
                    if in_quotes && quote_char == '"' {
                        if next_c == '$' || next_c == '"' || next_c == '\\' || next_c == '`' {
                            current_arg.push(next_c);
                        } else {
                            current_arg.push('\\');
                            current_arg.push(next_c);
                        }
                    } else {
                        current_arg.push(next_c);
                        was_quoted = true; // Treating escaped char as quoted to prevent globbing/splitting
                    }
                } else {
                    current_arg.push('\\');
                }
            }
            '"' if !in_quotes => {
                in_quotes = true;
                quote_char = '"';
                was_quoted = true;
            }
            '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = '\'';
                was_quoted = true;
            }
            c if in_quotes && c == quote_char => {
                in_quotes = false;
            }
            ' ' | '\t' if !in_quotes => {
                if !current_arg.is_empty() || was_quoted {
                    if was_quoted {
                        args.push(current_arg.clone());
                    } else {
                        let glob_expanded = expand_globs(&current_arg);
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
            '$' if !in_quotes || (in_quotes && quote_char == '"') => {
                if let Some(&next_char) = chars.peek() {
                    if next_char == '{' {
                        chars.next();
                        let mut var_name = String::new();
                        let mut found_closing = false;
                        while let Some(nc) = chars.next() {
                            if nc == '}' {
                                found_closing = true;
                                break;
                            }
                            var_name.push(nc);
                        }
                        if found_closing {
                            if let Some(value) = env_map.get(&var_name) {
                                current_arg.push_str(value);
                            }
                        } else {
                            current_arg.push_str("${");
                            current_arg.push_str(&var_name);
                        }
                    } else if next_char.is_alphabetic() || next_char == '_' {
                        let mut var_name = String::new();
                        while let Some(&nc) = chars.peek() {
                            if nc.is_alphanumeric() || nc == '_' {
                                var_name.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        }
                        if let Some(value) = env_map.get(&var_name) {
                            current_arg.push_str(value);
                        }
                    } else {
                        current_arg.push('$');
                    }
                } else {
                    current_arg.push('$');
                }
            }
            '~' if !in_quotes && current_arg.is_empty() => {
                let mut tilde_path = String::from("~");
                while let Some(&nc) = chars.peek() {
                    if nc == ' ' || nc == '\t' || nc == '/' || nc == '"' || nc == '\'' || nc == ';'
                    {
                        break;
                    }
                    tilde_path.push(chars.next().unwrap());
                }
                current_arg.push_str(&expand_tilde(&tilde_path));
            }
            _ => {
                current_arg.push(c);
            }
        }
    }

    if !current_arg.is_empty() || was_quoted {
        if was_quoted {
            args.push(current_arg);
        } else {
            let glob_expanded = expand_globs(&current_arg);
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
            '\\' if !in_quotes || (in_quotes && quote_char == '"') => {
                current_command.push(c);
                if let Some(next_c) = chars.next() {
                    current_command.push(next_c);
                }
            }
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

#[derive(Debug, PartialEq, Clone)]
pub enum Redirection {
    Stdout(String),
    StdoutAppend(String),
    Stderr(String),
    StderrAppend(String),
    Stdin(String),
}

#[derive(Debug)]
pub struct CommandArgs {
    pub args: Vec<String>,
    pub redirection: Vec<Redirection>,
}

pub fn parse_full_command(input: &str, env_map: &HashMap<String, String>) -> Vec<CommandArgs> {
    let mut commands = Vec::new();
    let mut current_args = Vec::new();
    let mut current_arg = String::new();
    let mut in_quotes = false;
    let mut quote_char = '"';
    let mut was_quoted = false;
    let mut redirections = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' if !in_quotes || (in_quotes && quote_char == '"') => {
                if let Some(next_c) = chars.next() {
                    if in_quotes && quote_char == '"' {
                        if next_c == '$' || next_c == '"' || next_c == '\\' || next_c == '`' {
                            current_arg.push(next_c);
                        } else {
                            current_arg.push('\\');
                            current_arg.push(next_c);
                        }
                    } else {
                        current_arg.push(next_c);
                        was_quoted = true;
                    }
                } else {
                    current_arg.push('\\');
                }
            }
            '"' if !in_quotes => {
                in_quotes = true;
                quote_char = '"';
                was_quoted = true;
            }
            '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = '\'';
                was_quoted = true;
            }
            c if in_quotes && c == quote_char => {
                in_quotes = false;
            }
            '|' if !in_quotes => {
                if !current_arg.is_empty() || was_quoted {
                    if was_quoted {
                        current_args.push(current_arg.clone());
                    } else {
                        current_args.extend(expand_globs(&current_arg));
                    }
                    current_arg.clear();
                    was_quoted = false;
                }
                commands.push(CommandArgs {
                    args: current_args.clone(),
                    redirection: redirections.clone(),
                });
                current_args.clear();
                redirections.clear();
            }
            '2' if !in_quotes && current_arg.is_empty() && chars.peek() == Some(&'>') => {
                chars.next(); // consume '>'
                let mut is_append = false;
                if chars.peek() == Some(&'>') {
                    is_append = true;
                    chars.next(); // consume second '>'
                }

                // Consume filename
                while let Some(&nc) = chars.peek() {
                    if nc == ' ' || nc == '\t' {
                        chars.next();
                    } else {
                        break;
                    }
                }
                let mut filename = String::new();
                while let Some(&nc) = chars.peek() {
                    if nc == ' ' || nc == '\t' || nc == '|' || nc == '>' || nc == ';' {
                        break;
                    }
                    filename.push(chars.next().unwrap());
                }

                if is_append {
                    redirections.push(Redirection::StderrAppend(filename));
                } else {
                    redirections.push(Redirection::Stderr(filename));
                }
            }
            '>' if !in_quotes => {
                if !current_arg.is_empty() || was_quoted {
                    if was_quoted {
                        current_args.push(current_arg.clone());
                    } else {
                        current_args.extend(expand_globs(&current_arg));
                    }
                    current_arg.clear();
                    was_quoted = false;
                }

                let mut is_append = false;
                if chars.peek() == Some(&'>') {
                    is_append = true;
                    chars.next(); // consume second '>'
                }

                // Consume filename
                while let Some(&nc) = chars.peek() {
                    if nc == ' ' || nc == '\t' {
                        chars.next();
                    } else {
                        break;
                    }
                }
                let mut filename = String::new();
                while let Some(&nc) = chars.peek() {
                    if nc == ' ' || nc == '\t' || nc == '|' || nc == '>' || nc == ';' {
                        break;
                    }
                    filename.push(chars.next().unwrap());
                }

                if is_append {
                    redirections.push(Redirection::StdoutAppend(filename));
                } else {
                    redirections.push(Redirection::Stdout(filename));
                }
            }
            '<' if !in_quotes => {
                if !current_arg.is_empty() || was_quoted {
                    if was_quoted {
                        current_args.push(current_arg.clone());
                    } else {
                        current_args.extend(expand_globs(&current_arg));
                    }
                    current_arg.clear();
                    was_quoted = false;
                }

                // Consume filename
                while let Some(&nc) = chars.peek() {
                    if nc == ' ' || nc == '\t' {
                        chars.next();
                    } else {
                        break;
                    }
                }
                let mut filename = String::new();
                while let Some(&nc) = chars.peek() {
                    if nc == ' ' || nc == '\t' || nc == '|' || nc == '>' || nc == '<' || nc == ';' {
                        break;
                    }
                    filename.push(chars.next().unwrap());
                }

                redirections.push(Redirection::Stdin(filename));
            }
            ' ' | '\t' if !in_quotes => {
                if !current_arg.is_empty() || was_quoted {
                    if was_quoted {
                        current_args.push(current_arg.clone());
                    } else {
                        current_args.extend(expand_globs(&current_arg));
                    }
                    current_arg.clear();
                    was_quoted = false;
                }
            }
            '$' if !in_quotes || (in_quotes && quote_char == '"') => {
                if let Some(&next_char) = chars.peek() {
                    if next_char == '{' {
                        chars.next();
                        let mut var_name = String::new();
                        let mut found_closing = false;
                        while let Some(nc) = chars.next() {
                            if nc == '}' {
                                found_closing = true;
                                break;
                            }
                            var_name.push(nc);
                        }
                        if found_closing {
                            if let Some(value) = env_map.get(&var_name) {
                                current_arg.push_str(value);
                            }
                        } else {
                            current_arg.push_str("${");
                            current_arg.push_str(&var_name);
                        }
                    } else if next_char.is_alphabetic() || next_char == '_' {
                        let mut var_name = String::new();
                        while let Some(&nc) = chars.peek() {
                            if nc.is_alphanumeric() || nc == '_' {
                                var_name.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        }
                        if let Some(value) = env_map.get(&var_name) {
                            current_arg.push_str(value);
                        }
                    } else {
                        current_arg.push('$');
                    }
                } else {
                    current_arg.push('$');
                }
            }
            '~' if !in_quotes && current_arg.is_empty() => {
                let mut tilde_path = String::from("~");
                while let Some(&nc) = chars.peek() {
                    if nc == ' '
                        || nc == '\t'
                        || nc == '/'
                        || nc == '"'
                        || nc == '\''
                        || nc == ';'
                        || nc == '|'
                        || nc == '>'
                    {
                        break;
                    }
                    tilde_path.push(chars.next().unwrap());
                }
                current_arg.push_str(&expand_tilde(&tilde_path));
            }
            _ => {
                current_arg.push(c);
            }
        }
    }

    if !current_arg.is_empty() || was_quoted {
        if was_quoted {
            current_args.push(current_arg);
        } else {
            current_args.extend(expand_globs(&current_arg));
        }
    }

    if !current_args.is_empty() || !redirections.is_empty() {
        commands.push(CommandArgs {
            args: current_args,
            redirection: redirections,
        });
    }

    commands
}
