mod commands;
mod completion;
mod parser;

use clap::{arg, command, value_parser};
use colored::*;
use commands::{execute_file_commands, execute_piped_commands, handle_builtin_command};
use completion::{ShellHelper, create_editor};
use parser::{parse_full_command, split_commands};
use rustyline::Editor;
use rustyline::error::ReadlineError;
use signal_hook::{consts::SIGINT, iterator::Signals};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

fn handle_line(
    rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    readline: Result<String, ReadlineError>,
    _history_file: &Path,
    aliases: &mut HashMap<String, String>,
) -> Result<bool, Box<dyn std::error::Error>> {
    match readline {
        Ok(line) => {
            rl.add_history_entry(line.as_str())?;
            let input = line.trim();

            if input.is_empty() {
                return Ok(true);
            }

            // Split by semicolons and execute each command
            let commands = split_commands(input);

            for cmd_input in commands {
                let cmd_input = cmd_input.trim();
                if cmd_input.is_empty() {
                    continue;
                }

                let full_commands = parse_full_command(cmd_input);
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

                    if let Some(should_continue) =
                        handle_builtin_command(command, &args, rl, aliases)?
                    {
                        if !should_continue {
                            return Ok(false);
                        }
                    } else {
                        execute_piped_commands(full_commands, aliases);
                    }
                } else {
                    execute_piped_commands(full_commands, aliases);
                }
            }

            Ok(true)
        }
        Err(ReadlineError::Interrupted) => Ok(true),
        Err(ReadlineError::Eof) => Ok(false),
        Err(err) => {
            eprintln!("{}: Error reading input: {err:?}", "Error".red().bold());
            Ok(false)
        }
    }
}

fn read_and_execute(
    rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    history_file: &Path,
    prompt: &Option<String>,
    aliases: &mut HashMap<String, String>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let current_dir = env::current_dir()?;
    let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let display_dir = if current_dir == home_dir {
        "~".to_string()
    } else if let Ok(stripped) = current_dir.strip_prefix(&home_dir) {
        format!("~/{}", stripped.display())
    } else {
        current_dir.display().to_string()
    };
    let default_prompt = format!("{}{} ", display_dir.bright_blue().bold(), ">".bold());

    let the_prompt = match &prompt {
        Some(cmd) => {
            let output = Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .output()?;

            match String::from_utf8(output.stdout) {
                Ok(prompt_str) => prompt_str,
                Err(_) => default_prompt,
            }
        }
        None => default_prompt,
    };

    let readline = rl.readline(&the_prompt);
    handle_line(rl, readline, history_file, aliases)
}

fn run_shell(
    history_file: PathBuf,
    prompt: Option<String>,
    file: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut signals = Signals::new([SIGINT])?;
    thread::spawn(move || for _sig in signals.forever() {});

    let mut rl = create_editor()?;

    if rl.load_history(&history_file).is_err() {
        println!("{}: No previous history.", "Info".blue().bold());
    }

    let mut aliases = HashMap::new();
    execute_file_commands(&file, &mut aliases)?;
    while read_and_execute(&mut rl, &history_file, &prompt, &mut aliases)? {}

    rl.save_history(&history_file)?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = command!()
        .arg(
            arg!(
                -H --history <FILE> "File to store command history"
            )
            .required(false)
            .value_parser(value_parser!(PathBuf)),
        )
        .arg(
            arg!(
                -p --prompt <CMD> "Command to execute for prompt"
            )
            .required(false)
            .value_parser(value_parser!(String)),
        )
        .arg(
            arg!(
                -f --file <FILE> "File to read commands from"
            )
            .required(false)
            .value_parser(value_parser!(PathBuf)),
        )
        .get_matches();

    let history_file = matches
        .get_one::<PathBuf>("history")
        .cloned()
        .unwrap_or_else(|| {
            let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
            home_dir.join("history.txt")
        });

    let prompt = matches.get_one::<String>("prompt").cloned();
    let file = matches.get_one::<PathBuf>("file").cloned().or_else(|| {
        let home_dir = dirs::home_dir()?;
        Some(home_dir.join(".shellrc"))
    });

    run_shell(history_file, prompt, file)
}
