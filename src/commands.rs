use crate::parser::{CommandArgs, Redirection, parse_arguments, parse_full_command};
use colored::*;
use rustyline::{Editor, history::FileHistory};
use std::collections::HashMap;
use std::env;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

static PREVIOUS_DIR: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
pub fn execute_command_with_redirection(
    command: &str,
    args: &[&str],
    redirections: &[Redirection],
    env_map: &HashMap<String, String>,
) {
    let mut cmd = Command::new(command);
    cmd.args(args);
    cmd.env_clear();
    cmd.envs(env_map);

    let mut stdout_redirected = false;
    let mut stderr_redirected = false;

    for redir in redirections {
        match redir {
            Redirection::Stdout(filename) => match File::create(filename) {
                Ok(file) => {
                    cmd.stdout(Stdio::from(file));
                    stdout_redirected = true;
                }
                Err(e) => {
                    eprintln!(
                        "{}: Failed to create file '{}': {}",
                        "Error".red().bold(),
                        filename,
                        e
                    );
                    return;
                }
            },
            Redirection::StdoutAppend(filename) => {
                match OpenOptions::new().create(true).append(true).open(filename) {
                    Ok(file) => {
                        cmd.stdout(Stdio::from(file));
                        stdout_redirected = true;
                    }
                    Err(e) => {
                        eprintln!(
                            "{}: Failed to open file '{}' for append: {}",
                            "Error".red().bold(),
                            filename,
                            e
                        );
                        return;
                    }
                }
            }
            Redirection::Stderr(filename) => match File::create(filename) {
                Ok(file) => {
                    cmd.stderr(Stdio::from(file));
                    stderr_redirected = true;
                }
                Err(e) => {
                    eprintln!(
                        "{}: Failed to create file '{}' for stderr: {}",
                        "Error".red().bold(),
                        filename,
                        e
                    );
                    return;
                }
            },
            Redirection::StderrAppend(filename) => {
                match OpenOptions::new().create(true).append(true).open(filename) {
                    Ok(file) => {
                        cmd.stderr(Stdio::from(file));
                        stderr_redirected = true;
                    }
                    Err(e) => {
                        eprintln!(
                            "{}: Failed to open file '{}' for stderr append: {}",
                            "Error".red().bold(),
                            filename,
                            e
                        );
                        return;
                    }
                }
            }
        }
    }

    if !stdout_redirected {
        cmd.stdout(Stdio::inherit());
    }
    if !stderr_redirected {
        cmd.stderr(Stdio::inherit());
    }

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("{}: {command}: {e}", "Error".red().bold());
            return;
        }
    };

    let status = child.wait();

    match status {
        Ok(status) => {
            if !status.success() {
                eprintln!(
                    "{}: Command exited with status: {status}",
                    "Warning".yellow().bold()
                );
            }
        }
        Err(e) => {
            eprintln!("{}: Failed to wait for command: {e}", "Error".red().bold());
        }
    }
}

pub fn execute_single_command(
    command_args: CommandArgs,
    aliases: &HashMap<String, String>,
    env_map: &mut HashMap<String, String>,
) {
    if command_args.args.is_empty() {
        return;
    }

    let command = &command_args.args[0];
    let args: Vec<&str> = command_args.args[1..].iter().map(|s| s.as_str()).collect();

    match command.as_str() {
        "set" => {
            if args.is_empty() {
                let mut vars: Vec<_> = env_map.iter().collect();
                vars.sort_by_key(|a| a.0);
                for (key, value) in vars {
                    println!("{}={}", key, value);
                }
            } else if args.len() == 1 && args[0].contains('=') {
                let env_def = args[0];
                if let Some(eq_pos) = env_def.find('=') {
                    let name = env_def[..eq_pos].to_string();
                    let value = env_def[eq_pos + 1..].to_string();
                    env_map.insert(name, value);
                }
            } else if args.len() == 2 {
                env_map.insert(args[0].to_string(), args[1].to_string());
            } else {
                eprintln!(
                    "{}: Usage: set [VAR=value] or set [VAR] [value]",
                    "set".red().bold()
                );
            }
        }
        "alias" => {
            if args.is_empty() {
                for (name, value) in aliases.iter() {
                    println!("alias {}=\"{}\"", name, value);
                }
            } else if args.len() == 1 && args[0].contains('=') {
                eprintln!(
                    "{}: Cannot modify aliases in this context",
                    "alias".yellow().bold()
                );
            } else {
                eprintln!("{}: Usage: alias [name=value]", "alias".red().bold());
            }
        }
        "cd" => {
            let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

            let target_dir = if args.is_empty() {
                dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
            } else if args[0] == "-" {
                let prev_dir_mutex = PREVIOUS_DIR.get_or_init(|| Mutex::new(None));
                if let Ok(prev_dir_guard) = prev_dir_mutex.lock() {
                    if let Some(prev_dir) = prev_dir_guard.as_ref() {
                        prev_dir.clone()
                    } else {
                        eprintln!("{}: -: No previous directory", "cd".red().bold());
                        return;
                    }
                } else {
                    eprintln!(
                        "{}: -: Failed to access previous directory",
                        "cd".red().bold()
                    );
                    return;
                }
            } else {
                let path = args[0];
                if path.starts_with("~") {
                    let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
                    if path == "~" {
                        home_dir
                    } else {
                        home_dir.join(&path[2..])
                    }
                } else {
                    PathBuf::from(path)
                }
            };

            if let Err(e) = env::set_current_dir(&target_dir) {
                eprintln!("{}: {}: {}", "cd".red().bold(), target_dir.display(), e);
            } else {
                let prev_dir_mutex = PREVIOUS_DIR.get_or_init(|| Mutex::new(None));
                if let Ok(mut prev_dir_guard) = prev_dir_mutex.lock() {
                    *prev_dir_guard = Some(current_dir);
                }

                if !args.is_empty() && args[0] == "-" {
                    println!("{}", target_dir.display());
                }
            }
        }
        _ => {
            let expanded_command = if let Some(alias_value) = aliases.get(command) {
                alias_value.clone()
            } else {
                command.to_string()
            };

            if expanded_command != *command {
                let expanded_parts = parse_arguments(&expanded_command, env_map);
                let mut final_args = expanded_parts.clone();
                final_args
                    .extend_from_slice(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
                let final_command = &final_args[0];
                let final_arg_refs: Vec<&str> =
                    final_args[1..].iter().map(|s| s.as_str()).collect();
                execute_command_with_redirection(
                    final_command,
                    &final_arg_refs,
                    &command_args.redirection,
                    env_map,
                );
            } else {
                execute_command_with_redirection(
                    command,
                    &args,
                    &command_args.redirection,
                    env_map,
                );
            }
        }
    }
}

pub fn execute_piped_commands(
    commands: Vec<CommandArgs>,
    aliases: &HashMap<String, String>,
    env_map: &mut HashMap<String, String>,
) {
    if commands.is_empty() {
        return;
    }

    if commands.len() == 1 {
        execute_single_command(commands.into_iter().next().unwrap(), aliases, env_map);
        return;
    }

    let mut children = Vec::new();
    let mut previous_stdout = None;

    for (i, cmd_args) in commands.iter().enumerate() {
        if cmd_args.args.is_empty() {
            continue;
        }

        let command = &cmd_args.args[0];
        let args: Vec<&str> = cmd_args.args[1..].iter().map(|s| s.as_str()).collect();

        let mut cmd = Command::new(command);
        cmd.args(args);
        cmd.env_clear();
        cmd.envs(&*env_map);

        if let Some(stdout) = previous_stdout.take() {
            cmd.stdin(stdout);
        }

        let mut stdout_redirected = false;
        let mut stderr_redirected = false;

        for redir in &cmd_args.redirection {
            match redir {
                Redirection::Stdout(filename) => {
                    if let Ok(file) = File::create(filename) {
                        cmd.stdout(Stdio::from(file));
                        stdout_redirected = true;
                    }
                }
                Redirection::StdoutAppend(filename) => {
                    if let Ok(file) = OpenOptions::new().create(true).append(true).open(filename) {
                        cmd.stdout(Stdio::from(file));
                        stdout_redirected = true;
                    }
                }
                Redirection::Stderr(filename) => {
                    if let Ok(file) = File::create(filename) {
                        cmd.stderr(Stdio::from(file));
                        stderr_redirected = true;
                    }
                }
                Redirection::StderrAppend(filename) => {
                    if let Ok(file) = OpenOptions::new().create(true).append(true).open(filename) {
                        cmd.stderr(Stdio::from(file));
                        stderr_redirected = true;
                    }
                }
            }
        }

        if !stdout_redirected {
            if i == commands.len() - 1 {
                cmd.stdout(Stdio::inherit());
            } else {
                cmd.stdout(Stdio::piped());
            }
        }

        if !stderr_redirected {
            cmd.stderr(Stdio::inherit());
        }

        match cmd.spawn() {
            Ok(mut child) => {
                previous_stdout = child.stdout.take();
                children.push(child);
            }
            Err(e) => {
                eprintln!("{}: {command}: {e}", "Error".red().bold());
                return;
            }
        }
    }

    for mut child in children {
        match child.wait() {
            Ok(status) => {
                if !status.success() {
                    eprintln!(
                        "{}: Command exited with status: {status}",
                        "Warning".yellow().bold()
                    );
                }
            }
            Err(e) => {
                eprintln!("{}: Failed to wait for command: {e}", "Error".red().bold());
            }
        }
    }
}

pub fn handle_builtin_command(
    command: &str,
    args: &[&str],
    rl: &mut Editor<crate::completion::ShellHelper, FileHistory>,
    aliases: &mut HashMap<String, String>,
    env_map: &mut HashMap<String, String>,
) -> Result<Option<bool>, Box<dyn std::error::Error>> {
    match command {
        "exit" => Ok(Some(false)),
        "help" => {
            println!("{}", "Shell Builtin Commands:".bold().bright_blue());
            println!("  {:10} - Change the current directory", "cd".green());
            println!("  {:10} - Set or list environment variables", "set".green());
            println!("  {:10} - Define or list aliases", "alias".green());
            println!(
                "  {:10} - Add a directory to PATH or list PATH",
                "path".green()
            );
            println!("  {:10} - Display the command history", "history".green());
            println!(
                "  {:10} - Edit the last or a specific command",
                "edit".green()
            );
            println!("  {:10} - Exit the shell", "exit".green());
            println!("  {:10} - Display this help message", "help".green());
            println!("\n{}", "Usage Hints:".bold().bright_blue());
            println!("  - Use | for piping commands");
            println!("  - Use > or >> for output redirection");
            println!("  - Use 2> or 2>> for error redirection");
            println!("  - Environment variables: $VAR or ${{VAR}}");
            println!("  - Tilde expansion: ~/path");
            println!("  - Wildcards: *, ?, [a-z]");
            Ok(Some(true))
        }
        "history" => {
            for (i, entry) in rl.history().iter().enumerate() {
                println!("{:5}  {}", i + 1, entry);
            }
            Ok(Some(true))
        }
        "alias" => {
            if args.is_empty() {
                for (name, value) in aliases.iter() {
                    println!("alias {}=\"{}\"", name, value);
                }
            } else if args.len() == 1 && args[0].contains('=') {
                let alias_def = args[0];
                if let Some(eq_pos) = alias_def.find('=') {
                    let name = alias_def[..eq_pos].to_string();
                    let value = alias_def[eq_pos + 1..].trim_matches('"').to_string();
                    aliases.insert(name, value);
                }
            } else {
                eprintln!("{}: Usage: alias [name=value]", "alias".red().bold());
            }
            Ok(Some(true))
        }
        "path" => {
            if args.is_empty() {
                if let Some(path) = env_map.get("PATH") {
                    println!("{}", path);
                } else {
                    println!();
                }
            } else if args.len() == 1 {
                let new_path = args[0];
                let expanded_path = if new_path.starts_with("~") {
                    let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
                    home_dir.join(&new_path[2..]).to_string_lossy().to_string()
                } else {
                    new_path.to_string()
                };

                let path_buf = PathBuf::from(&expanded_path);
                if !path_buf.exists() {
                    eprintln!(
                        "{}: Directory does not exist: {}",
                        "path".red().bold(),
                        expanded_path
                    );
                } else if !path_buf.is_dir() {
                    eprintln!(
                        "{}: Not a directory: {}",
                        "path".red().bold(),
                        expanded_path
                    );
                } else {
                    let current_path = env_map.get("PATH").cloned().unwrap_or_default();
                    let new_full_path = if current_path.is_empty() {
                        expanded_path.clone()
                    } else {
                        format!("{}:{}", expanded_path, current_path)
                    };
                    env_map.insert("PATH".to_string(), new_full_path);
                    println!("{}: Added {} to PATH", "path".green().bold(), expanded_path);
                }
            } else {
                eprintln!("{}: Usage: path [directory]", "path".red().bold());
            }
            Ok(Some(true))
        }
        "edit" => {
            let editor = env_map
                .get("EDITOR")
                .cloned()
                .unwrap_or_else(|| "vim".to_string());
            let last_command = if args.is_empty() {
                rl.history()
                    .into_iter()
                    .rev()
                    .nth(1)
                    .map(|entry| entry.to_string())
            } else {
                Some(args.join(" "))
            };

            if let Some(cmd) = last_command {
                let mut temp_file = tempfile::NamedTempFile::new()?;
                use std::io::Write;
                temp_file.write_all(cmd.as_bytes())?;

                let temp_path = temp_file.path().to_owned();
                let status = Command::new(editor).arg(&temp_path).status()?;

                if status.success() {
                    let edited_command = std::fs::read_to_string(&temp_path)?;
                    let full_commands = parse_full_command(edited_command.trim(), env_map);
                    execute_piped_commands(full_commands, aliases, env_map);
                } else {
                    eprintln!(
                        "{}: Editor exited with status: {}",
                        "Warning".yellow().bold(),
                        status
                    );
                }
            } else {
                eprintln!("{}: No previous command to edit.", "Info".blue().bold());
            }
            Ok(Some(true))
        }
        _ => Ok(None),
    }
}

pub fn execute_file_commands(
    file: &Option<PathBuf>,
    aliases: &mut HashMap<String, String>,
    env_map: &mut HashMap<String, String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(file_path) = file {
        if file_path.exists() {
            let content = std::fs::read_to_string(file_path)?;
            for line in content.lines() {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }

                let full_commands = parse_full_command(input, env_map);
                if full_commands.is_empty() {
                    continue;
                }

                if full_commands.len() == 1 {
                    let cmd_args = &full_commands[0];
                    if cmd_args.args.is_empty() {
                        continue;
                    }
                    let command = &cmd_args.args[0];
                    let args: Vec<&str> = cmd_args.args[1..].iter().map(|s| s.as_str()).collect();

                    match command.as_str() {
                        "exit" => break,
                        "alias" => {
                            if args.is_empty() {
                                for (name, value) in aliases.iter() {
                                    println!("alias {}=\"{}\"", name, value);
                                }
                            } else if args.len() == 1 && args[0].contains('=') {
                                let alias_def = args[0];
                                if let Some(eq_pos) = alias_def.find('=') {
                                    let name = alias_def[..eq_pos].to_string();
                                    let value =
                                        alias_def[eq_pos + 1..].trim_matches('"').to_string();
                                    aliases.insert(name, value);
                                }
                            } else {
                                eprintln!("{}: Usage: alias [name=value]", "alias".red().bold());
                            }
                        }
                        "path" => {
                            if args.is_empty() {
                                if let Some(path) = env_map.get("PATH") {
                                    println!("{}", path);
                                } else {
                                    println!();
                                }
                            } else if args.len() == 1 {
                                let new_path = args[0];
                                let expanded_path = if new_path.starts_with("~") {
                                    let home_dir =
                                        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
                                    home_dir.join(&new_path[2..]).to_string_lossy().to_string()
                                } else {
                                    new_path.to_string()
                                };

                                let path_buf = PathBuf::from(&expanded_path);
                                if !path_buf.exists() {
                                    eprintln!(
                                        "{}: Directory does not exist: {}",
                                        "path".red().bold(),
                                        expanded_path
                                    );
                                } else if !path_buf.is_dir() {
                                    eprintln!(
                                        "{}: Not a directory: {}",
                                        "path".red().bold(),
                                        expanded_path
                                    );
                                } else {
                                    let current_path =
                                        env_map.get("PATH").cloned().unwrap_or_default();
                                    let new_full_path = if current_path.is_empty() {
                                        expanded_path.clone()
                                    } else {
                                        format!("{}:{}", expanded_path, current_path)
                                    };
                                    env_map.insert("PATH".to_string(), new_full_path);
                                    println!(
                                        "{}: Added {} to PATH",
                                        "path".green().bold(),
                                        expanded_path
                                    );
                                }
                            } else {
                                eprintln!("{}: Usage: path [directory]", "path".red().bold());
                            }
                        }
                        _ => {
                            execute_single_command(
                                full_commands.into_iter().next().unwrap(),
                                aliases,
                                env_map,
                            );
                        }
                    }
                } else {
                    execute_piped_commands(full_commands, aliases, env_map);
                }
            }
        } else {
            eprintln!(
                "{}: File not found: {}",
                "Error".red().bold(),
                file_path.display()
            );
        }
    }
    Ok(())
}
