use crate::parser::{
    expand_tilde, parse_arguments, parse_full_command, split_conditional_commands, CommandArgs,
    LogicalOperator, Redirection,
};
use colored::*;
use rustyline::{history::FileHistory, Editor};
use std::collections::HashMap;
use std::env;
use std::fs::{File, OpenOptions};
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

static PREVIOUS_DIR: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

#[derive(Debug, PartialEq)]
pub enum ExecutionResult {
    Success,
    Failure,
    Interrupted,
}

pub fn execute_command_with_redirection(
    command: &str,
    args: &[&str],
    redirections: &[Redirection],
    env_map: &HashMap<String, String>,
    background: bool,
) -> ExecutionResult {
    let mut cmd = Command::new(command);
    cmd.args(args);
    cmd.env_clear();
    cmd.envs(env_map);

    let mut stdout_redirected = false;
    let mut stderr_redirected = false;
    let mut stdin_redirected = false;

    for redir in redirections {
        match redir {
            Redirection::Stdin(filename) => match File::open(filename) {
                Ok(file) => {
                    cmd.stdin(Stdio::from(file));
                    stdin_redirected = true;
                }
                Err(e) => {
                    eprintln!(
                        "{}: Failed to open file '{}' for stdin: {}",
                        "Error".red().bold(),
                        filename,
                        e
                    );
                    return ExecutionResult::Failure;
                }
            },
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
                    return ExecutionResult::Failure;
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
                        return ExecutionResult::Failure;
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
                    return ExecutionResult::Failure;
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
                        return ExecutionResult::Failure;
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
    if !stdin_redirected {
        cmd.stdin(Stdio::inherit());
    }

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                if command.contains('/') {
                    eprintln!(
                        "{}: {}: No such file or directory",
                        "Error".red().bold(),
                        command
                    );
                } else {
                    eprintln!("{}: command not found", command);
                }
            } else {
                eprintln!("{}: {command}: {e}", "Error".red().bold());
            }
            return ExecutionResult::Failure;
        }
    };

    if background {
        println!("[{}]", child.id());
        return ExecutionResult::Success;
    }

    let status = child.wait();

    match status {
        Ok(status) => {
            if let Some(sig) = status.signal() {
                if sig == 2 {
                    // SIGINT
                    println!(); // Just a newline to clean up
                    return ExecutionResult::Interrupted;
                }
                eprintln!(
                    "{}: Command terminated by signal: {}",
                    "Warning".yellow().bold(),
                    sig
                );
                return ExecutionResult::Interrupted;
            }

            if !status.success() {
                return ExecutionResult::Failure;
            }
            ExecutionResult::Success
        }
        Err(e) => {
            eprintln!("{}: Failed to wait for command: {e}", "Error".red().bold());
            ExecutionResult::Failure
        }
    }
}

pub fn execute_single_command(
    command_args: CommandArgs,
    aliases: &mut HashMap<String, String>,
    env_map: &mut HashMap<String, String>,
) -> ExecutionResult {
    if command_args.args.is_empty() {
        return ExecutionResult::Success;
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
                return ExecutionResult::Failure;
            }
            ExecutionResult::Success
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
                return ExecutionResult::Failure;
            }
            ExecutionResult::Success
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
                        return ExecutionResult::Failure;
                    }
                } else {
                    eprintln!(
                        "{}: -: Failed to access previous directory",
                        "cd".red().bold()
                    );
                    return ExecutionResult::Failure;
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
                ExecutionResult::Failure
            } else {
                let prev_dir_mutex = PREVIOUS_DIR.get_or_init(|| Mutex::new(None));
                if let Ok(mut prev_dir_guard) = prev_dir_mutex.lock() {
                    *prev_dir_guard = Some(current_dir);
                }

                if !args.is_empty() && args[0] == "-" {
                    println!("{}", target_dir.display());
                }
                ExecutionResult::Success
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
                    command_args.background,
                )
            } else {
                execute_command_with_redirection(
                    command,
                    &args,
                    &command_args.redirection,
                    env_map,
                    command_args.background,
                )
            }
        }
    }
}

pub fn execute_piped_commands(
    commands: Vec<CommandArgs>,
    aliases: &mut HashMap<String, String>,
    env_map: &mut HashMap<String, String>,
) -> ExecutionResult {
    if commands.is_empty() {
        return ExecutionResult::Success;
    }

    if commands.len() == 1 {
        return execute_single_command(commands.into_iter().next().unwrap(), aliases, env_map);
    }

    let background = commands[0].background;
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
        let mut stdin_redirected = false;

        for redir in &cmd_args.redirection {
            match redir {
                Redirection::Stdin(filename) => {
                    if let Ok(file) = File::open(filename) {
                        cmd.stdin(Stdio::from(file));
                        stdin_redirected = true;
                    } else {
                        eprintln!(
                            "{}: Failed to open file '{}' for stdin",
                            "Error".red().bold(),
                            filename
                        );
                        return ExecutionResult::Failure;
                    }
                }
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

        if !stdin_redirected && i == 0 {
            cmd.stdin(Stdio::inherit());
        }

        match cmd.spawn() {
            Ok(mut child) => {
                previous_stdout = child.stdout.take();
                children.push(child);
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    if command.contains('/') {
                        eprintln!(
                            "{}: {}: No such file or directory",
                            "Error".red().bold(),
                            command
                        );
                    } else {
                        eprintln!("{}: command not found", command);
                    }
                } else {
                    eprintln!("{}: {command}: {e}", "Error".red().bold());
                }
                return ExecutionResult::Failure;
            }
        }
    }

    if background {
        for child in &children {
            println!("[{}]", child.id());
        }
        return ExecutionResult::Success;
    }

    let mut interrupted = false;
    let mut failure = false;

    for mut child in children {
        match child.wait() {
            Ok(status) => {
                if let Some(sig) = status.signal() {
                    if sig == 2 {
                        interrupted = true;
                    } else if !interrupted {
                        eprintln!(
                            "{}: Command terminated by signal: {}",
                            "Warning".yellow().bold(),
                            sig
                        );
                        interrupted = true;
                    }
                } else if !status.success() && !interrupted {
                    failure = true;
                }
            }
            Err(e) => {
                eprintln!("{}: Failed to wait for command: {e}", "Error".red().bold());
                failure = true;
            }
        }
    }

    if interrupted {
        println!();
        ExecutionResult::Interrupted
    } else if failure {
        ExecutionResult::Failure
    } else {
        ExecutionResult::Success
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
            println!("  - Use < for input redirection");
            println!("  - Use && and || for logical operators");
            println!("  - Use & to run commands in the background");
            println!("  - Environment variables: $VAR or ${{VAR}}");
            println!(r"  - Command substitution: $(command) or `command` ");
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
                let expanded_path = expand_tilde(new_path);

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
                if input.is_empty() || input.starts_with('#') {
                    continue;
                }

                let conditional_commands = split_conditional_commands(input);
                let mut skip_until_next_semicolon = false;

                for cond_cmd in conditional_commands {
                    if skip_until_next_semicolon {
                        if cond_cmd.operator == LogicalOperator::Semicolon {
                            skip_until_next_semicolon = false;
                        }
                        continue;
                    }

                    let cmd_input = cond_cmd.command.trim();
                    if cmd_input.is_empty() {
                        continue;
                    }

                    let full_commands = parse_full_command(cmd_input, env_map);
                    if full_commands.is_empty() {
                        continue;
                    }

                    let current_result = if full_commands.len() == 1 {
                        let cmd_args = &full_commands[0];
                        if cmd_args.args.is_empty() {
                            ExecutionResult::Success
                        } else {
                            let command = &cmd_args.args[0];
                            let args: Vec<&str> =
                                cmd_args.args[1..].iter().map(|s| s.as_str()).collect();

                            match command.as_str() {
                                "exit" => return Ok(()),
                                "set" => execute_single_command(
                                    full_commands.into_iter().next().unwrap(),
                                    aliases,
                                    env_map,
                                ),
                                "alias" => execute_single_command(
                                    full_commands.into_iter().next().unwrap(),
                                    aliases,
                                    env_map,
                                ),
                                "cd" => execute_single_command(
                                    full_commands.into_iter().next().unwrap(),
                                    aliases,
                                    env_map,
                                ),
                                "path" => {
                                    if args.is_empty() {
                                        if let Some(path) = env_map.get("PATH") {
                                            println!("{}", path);
                                        } else {
                                            println!();
                                        }
                                    } else if args.len() == 1 {
                                        let new_path = args[0];
                                        let expanded_path = expand_tilde(new_path);

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
                                        eprintln!(
                                            "{}: Usage: path [directory]",
                                            "path".red().bold()
                                        );
                                    }
                                    ExecutionResult::Success
                                }
                                _ => execute_piped_commands(full_commands, aliases, env_map),
                            }
                        }
                    } else {
                        execute_piped_commands(full_commands, aliases, env_map)
                    };

                    if current_result == ExecutionResult::Interrupted {
                        break;
                    }

                    match cond_cmd.operator {
                        LogicalOperator::And => {
                            if current_result == ExecutionResult::Failure {
                                skip_until_next_semicolon = true;
                            }
                        }
                        LogicalOperator::Or => {
                            if current_result == ExecutionResult::Success {
                                skip_until_next_semicolon = true;
                            }
                        }
                        _ => {}
                    }
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
