use std::env;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

fn main() {
    loop {
        print!("> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        let command = parts[0];
        let args = &parts[1..];

        match command {
            _ => {
                execute_command(command, args);
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
