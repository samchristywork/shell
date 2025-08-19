use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use serde::Deserialize;
use signal_hook::{consts::SIGINT, iterator::Signals};
use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

#[derive(Deserialize, Default)]
struct Config {
    history_file: Option<String>,
    prompt: Option<String>,
    prompt_cmd: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_content = match fs::read_to_string("config.toml") {
        Ok(content) => content,
        Err(_e) => {
            println!("Using default configuration.");
            "".to_string()
        }
    };
    let config: Config = toml::from_str(&config_content).unwrap_or_default();

    let history_file = config
        .history_file
        .clone()
        .unwrap_or_else(|| "history.txt".to_string());
    let prompt = config.prompt.clone();
    let prompt_cmd = config.prompt_cmd.clone();

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
                continue;
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
