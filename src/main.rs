use clap::{arg, command, value_parser};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use signal_hook::{consts::SIGINT, iterator::Signals};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

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
                -p --promptcmd <CMD> "Command to execute for prompt"
            )
            .required(false)
            .value_parser(value_parser!(String)),
        )
        .arg(
            arg!(
                -P --prompt <PROMPT> "Custom prompt string"
            )
            .required(false)
            .value_parser(value_parser!(String)),
        )
        .get_matches();

    let history_file = matches
        .get_one::<PathBuf>("history")
        .cloned()
        .unwrap_or_else(|| {
            let home_dir = env::home_dir().unwrap_or_else(|| PathBuf::from("/"));
            home_dir.join("history.txt")
        });

    let prompt_cmd = matches.get_one::<String>("promptcmd").cloned();
    let prompt = matches.get_one::<String>("prompt").cloned();

    let mut signals = Signals::new([SIGINT])?;
    thread::spawn(move || for _sig in signals.forever() {});

    let mut rl = DefaultEditor::new()?;
    if rl.load_history(&history_file).is_err() {
        println!("No previous history.");
    }

    loop {
        let the_prompt = match &prompt_cmd {
            Some(cmd) => {
                let output = Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::inherit())
                    .output()?;

                String::from_utf8(output.stdout).unwrap_or_else(|_| "> ".to_string())
            }
            None => prompt.clone().unwrap_or_else(|| "> ".to_string()),
        };

        let readline = rl.readline(&the_prompt);
        match readline {
            Ok(line) => {
                rl.add_history_entry(line.as_str())?;

                let input = line.trim();

                if input.is_empty() {
                    continue;
                }

                let parts: Vec<&str> = input.split_whitespace().collect();
                let command = parts[0];
                let args = &parts[1..];

                match command {
                    "exit" => break,
                    "cd" => {
                        let target_dir = if args.is_empty() {
                            env::home_dir().unwrap_or_else(|| Path::new("/").to_path_buf())
                        } else {
                            Path::new(args[0]).to_path_buf()
                        };

                        if let Err(e) = env::set_current_dir(&target_dir) {
                            eprintln!("cd: {}: {}", target_dir.display(), e);
                        }
                    }
                    _ => {
                        execute_command(command, args);
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
            }
            Err(ReadlineError::Eof) => {
                break;
            }
            Err(err) => {
                eprintln!("Error reading input: {err:?}");
                break;
            }
        }
    }
    rl.save_history(&history_file)?;

    Ok(())
}

fn execute_command(command: &str, args: &[&str]) {
    let mut cmd = Command::new(command);
    cmd.args(args);
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("Failed to execute {command}: {e}");
            return;
        }
    };

    let status = child.wait();

    match status {
        Ok(status) => {
            if !status.success() {
                eprintln!("Command exited with status: {status}");
            }
        }
        Err(e) => {
            eprintln!("Failed to wait for command: {e}");
        }
    }
}
