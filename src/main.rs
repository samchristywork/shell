use std::env;
use std::io::{self, Write, BufRead};
use std::path::Path;
use std::process::{Command, Stdio};

fn main() {
    loop {
        print!("> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        let stdin = io::stdin();
        let mut reader = stdin.lock();

        match reader.read_line(&mut input) {
            Ok(0) => {
                break;
            }
            Ok(_) => {
                let input = input.trim();

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
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
        }
    }
}

fn execute_command(command: &str, args: &[&str]) {
    let mut cmd = Command::new(command);
    cmd.args(args);
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("Failed to execute {}: {}", command, e);
            return;
        }
    };

    let status = child.wait();

    match status {
        Ok(status) => {
            if !status.success() {
                eprintln!("Command exited with status: {}", status);
            }
        }
        Err(e) => {
            eprintln!("Failed to wait for command: {}", e);
        }
    }
}
