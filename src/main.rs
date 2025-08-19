use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use signal_hook::{consts::SIGINT, iterator::Signals};
use std::env;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

fn main() {
    let mut signals = Signals::new([SIGINT]).unwrap();
    thread::spawn(move || for _sig in signals.forever() {});

    let mut rl = DefaultEditor::new().unwrap();
    if rl.load_history("history.txt").is_err() {
        println!("No previous history.");
    }

    loop {
        let readline = rl.readline("> ");
        match readline {
            Ok(line) => {
                rl.add_history_entry(line.as_str()).unwrap();

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
                            env::home_dir().unwrap()
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
    rl.save_history("history.txt").unwrap();
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
