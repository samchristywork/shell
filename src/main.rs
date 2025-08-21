use clap::{arg, command, value_parser};
use colored::*;
use rustyline::completion::{Completer, Pair};
use rustyline::config::Config;
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter, MatchingBracketHighlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::{MatchingBracketValidator, Validator};
use rustyline::{CompletionType, Helper};
use rustyline::{Context, Editor};
use signal_hook::{consts::SIGINT, iterator::Signals};
use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

struct ShellHelper {
    completer: ShellCompleter,
    hinter: HistoryHinter,
    validator: MatchingBracketValidator,
    highlighter: MatchingBracketHighlighter,
}

impl ShellHelper {
    fn new() -> ShellHelper {
        ShellHelper {
            completer: ShellCompleter::new(),
            hinter: HistoryHinter::new(),
            validator: MatchingBracketValidator::new(),
            highlighter: MatchingBracketHighlighter::new(),
        }
    }
}

impl Helper for ShellHelper {}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        self.completer.complete(line, pos, ctx)
    }
}

impl Hinter for ShellHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<String> {
        self.hinter.hint(line, pos, ctx)
    }
}

impl Validator for ShellHelper {
    fn validate(
        &self,
        ctx: &mut rustyline::validate::ValidationContext,
    ) -> rustyline::Result<rustyline::validate::ValidationResult> {
        self.validator.validate(ctx)
    }

    fn validate_while_typing(&self) -> bool {
        self.validator.validate_while_typing()
    }
}

impl Highlighter for ShellHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        default: bool,
    ) -> Cow<'b, str> {
        self.highlighter.highlight_prompt(prompt, default)
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        self.highlighter.highlight_hint(hint)
    }

    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        self.highlighter.highlight(line, pos)
    }

    fn highlight_char(&self, line: &str, pos: usize, kind: CmdKind) -> bool {
        self.highlighter.highlight_char(line, pos, kind)
    }
}

struct ShellCompleter;

impl ShellCompleter {
    fn new() -> ShellCompleter {
        ShellCompleter
    }

    fn get_builtin_commands() -> Vec<String> {
        vec![
            "cd".to_string(),
            "edit".to_string(),
            "exit".to_string(),
            "alias".to_string(),
        ]
    }

    fn get_path_commands() -> Vec<String> {
        let mut commands = Vec::new();

        if let Ok(path_var) = env::var("PATH") {
            for path in path_var.split(':') {
                if let Ok(entries) = std::fs::read_dir(path) {
                    for entry in entries.flatten() {
                        if let Ok(metadata) = entry.metadata() {
                            if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 {
                                if let Some(name) = entry.file_name().to_str() {
                                    commands.push(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        commands.sort();
        commands.dedup();
        commands
    }

    fn get_filename_completions(partial_path: &str) -> Vec<Pair> {
        let mut candidates = Vec::new();

        let expanded_path = if partial_path.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(&partial_path[2..]).to_string_lossy().to_string()
            } else {
                partial_path.to_string()
            }
        } else if partial_path == "~" {
            if let Some(home) = dirs::home_dir() {
                home.to_string_lossy().to_string()
            } else {
                partial_path.to_string()
            }
        } else {
            partial_path.to_string()
        };

        let path = Path::new(&expanded_path);
        let (dir_path, filename_prefix) = if expanded_path.ends_with('/') {
            (path, "")
        } else if expanded_path.contains('/') {
            match path.parent() {
                Some(parent) => (
                    parent,
                    path.file_name().unwrap_or_default().to_str().unwrap_or(""),
                ),
                None => (Path::new("."), expanded_path.as_str()),
            }
        } else {
            (Path::new("."), expanded_path.as_str())
        };

        if let Ok(entries) = std::fs::read_dir(dir_path) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(filename_prefix) {
                        let is_dir = entry.file_type().map_or(false, |ft| ft.is_dir());
                        let display_name = if is_dir {
                            format!("{}/", name)
                        } else {
                            name.to_string()
                        };

                        let base_replacement = if partial_path.starts_with("~/") {
                            if dir_path == dirs::home_dir().unwrap_or_default() {
                                format!("~/{}", name)
                            } else {
                                let relative_dir = dir_path
                                    .strip_prefix(dirs::home_dir().unwrap_or_default())
                                    .unwrap_or(dir_path);
                                if relative_dir == Path::new("") {
                                    format!("~/{}", name)
                                } else {
                                    format!("~/{}/{}", relative_dir.display(), name)
                                }
                            }
                        } else if dir_path == Path::new(".") {
                            name.to_string()
                        } else if expanded_path.ends_with('/') {
                            format!("{}{}", expanded_path, name)
                        } else if expanded_path.contains('/') {
                            format!("{}/{}", dir_path.display(), name)
                        } else {
                            name.to_string()
                        };

                        let replacement = if is_dir {
                            format!("{}/", base_replacement)
                        } else {
                            base_replacement
                        };

                        candidates.push(Pair {
                            display: display_name,
                            replacement,
                        });
                    }
                }
            }
        }

        candidates.sort_by(|a, b| a.display.cmp(&b.display));
        candidates
    }
}

impl Completer for ShellCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let words: Vec<&str> = line[..pos].split_whitespace().collect();

        if words.is_empty() || (words.len() == 1 && !line[..pos].ends_with(' ')) {
            let word_to_complete = if words.is_empty() { "" } else { words[0] };

            let mut candidates = Vec::new();

            for cmd in Self::get_builtin_commands() {
                if cmd.starts_with(word_to_complete) {
                    candidates.push(Pair {
                        display: format!("{} {}", cmd, "(builtin)".bright_black()),
                        replacement: cmd,
                    });
                }
            }

            for cmd in Self::get_path_commands() {
                if cmd.starts_with(word_to_complete) {
                    candidates.push(Pair {
                        display: cmd.clone(),
                        replacement: cmd,
                    });
                }
            }

            let start = pos - word_to_complete.len();
            Ok((start, candidates))
        } else {
            let current_word_start = line[..pos].rfind(' ').map_or(0, |i| i + 1);
            let word_to_complete = &line[current_word_start..pos];

            let candidates = Self::get_filename_completions(word_to_complete);
            Ok((current_word_start, candidates))
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

fn expand_tilde(path: &str) -> String {
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

fn parse_arguments(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current_arg = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' if !in_quotes => {
                in_quotes = true;
            }
            '"' if in_quotes => {
                in_quotes = false;
            }
            ' ' | '\t' if !in_quotes => {
                if !current_arg.is_empty() {
                    args.push(expand_tilde(&current_arg));
                    current_arg.clear();
                }
                // Skip multiple spaces
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
        args.push(expand_tilde(&current_arg));
    }

    args
}

fn execute_piped_commands(commands: Vec<Vec<String>>) {
    if commands.is_empty() {
        return;
    }

    if commands.len() == 1 {
        let cmd = &commands[0];
        if !cmd.is_empty() {
            let cmd_args: Vec<&str> = cmd[1..].iter().map(|s| s.as_str()).collect();
            execute_command(&cmd[0], &cmd_args);
        }
        return;
    }

    let mut children = Vec::new();
    let mut previous_stdout = None;

    for (i, cmd_parts) in commands.iter().enumerate() {
        if cmd_parts.is_empty() {
            continue;
        }

        let command = &cmd_parts[0];
        let args: Vec<&str> = cmd_parts[1..].iter().map(|s| s.as_str()).collect();

        let mut cmd = Command::new(command);
        cmd.args(args);

        if let Some(stdout) = previous_stdout.take() {
            cmd.stdin(stdout);
        }

        if i == commands.len() - 1 {
            cmd.stdout(Stdio::inherit());
        } else {
            cmd.stdout(Stdio::piped());
        }

        cmd.stderr(Stdio::inherit());

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

            let parts = parse_arguments(input);
            let command = &parts[0];
            let args: Vec<&str> = parts[1..].iter().map(|s| s.as_str()).collect();

            match command.as_str() {
                "exit" => return Ok(false),
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
                }
                "edit" => {
                    let editor = env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
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
                        let temp_file_path = Path::new("/tmp/last_command");
                        std::fs::write(temp_file_path, cmd)?;
                        let status = Command::new(editor).arg(temp_file_path).status()?;
                        if status.success() {
                            let edited_command = std::fs::read_to_string(temp_file_path)?;
                            let edited_parts = parse_arguments(edited_command.trim());
                            if !edited_parts.is_empty() {
                                let edited_cmd = &edited_parts[0];
                                let edited_args: Vec<&str> =
                                    edited_parts[1..].iter().map(|s| s.as_str()).collect();
                                execute_command(edited_cmd, &edited_args);
                            }
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
                }
                "cd" => {
                    let target_dir = if args.is_empty() {
                        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
                    } else {
                        PathBuf::from(args[0])
                    };

                    if let Err(e) = env::set_current_dir(&target_dir) {
                        eprintln!("{}: {}: {}", "cd".red().bold(), target_dir.display(), e);
                    }
                }
                _ => {
                    let expanded_command = if let Some(alias_value) = aliases.get(command) {
                        alias_value.clone()
                    } else {
                        command.clone()
                    };

                    if input.contains('|') {
                        let pipe_parts: Vec<&str> = input.split('|').collect();
                        let commands: Vec<Vec<String>> = pipe_parts
                            .iter()
                            .map(|part| {
                                let mut parsed = parse_arguments(part.trim());
                                if !parsed.is_empty() {
                                    if let Some(alias_value) = aliases.get(&parsed[0]) {
                                        let alias_parts = parse_arguments(alias_value);
                                        parsed.splice(0..1, alias_parts);
                                    }
                                }
                                parsed
                            })
                            .collect();
                        execute_piped_commands(commands);
                    } else {
                        if expanded_command != *command {
                            let expanded_parts = parse_arguments(&expanded_command);
                            let mut final_args = expanded_parts.clone();
                            final_args.extend_from_slice(
                                &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                            );
                            let final_command = &final_args[0];
                            let final_arg_refs: Vec<&str> =
                                final_args[1..].iter().map(|s| s.as_str()).collect();
                            execute_command(final_command, &final_arg_refs);
                        } else {
                            execute_command(command, &args);
                        }
                    }
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
    let default_prompt = format!(
        "{}> ",
        env::current_dir()?.display().to_string().blue().bold()
    );

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

fn execute_file_commands(
    file: &Option<PathBuf>,
    aliases: &mut HashMap<String, String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(file_path) = file {
        if file_path.exists() {
            let content = std::fs::read_to_string(file_path)?;
            for line in content.lines() {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }

                let parts = parse_arguments(input);
                if parts.is_empty() {
                    continue;
                }

                let command = &parts[0];
                let args: Vec<&str> = parts[1..].iter().map(|s| s.as_str()).collect();

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
                                let value = alias_def[eq_pos + 1..].trim_matches('"').to_string();
                                aliases.insert(name, value);
                            }
                        } else {
                            eprintln!("{}: Usage: alias [name=value]", "alias".red().bold());
                        }
                    }
                    "cd" => {
                        let target_dir = if args.is_empty() {
                            dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
                        } else {
                            PathBuf::from(args[0])
                        };

                        if let Err(e) = env::set_current_dir(&target_dir) {
                            eprintln!("{}: {}: {}", "cd".red().bold(), target_dir.display(), e);
                        }
                    }
                    _ => {
                        let expanded_command = if let Some(alias_value) = aliases.get(command) {
                            alias_value.clone()
                        } else {
                            command.clone()
                        };

                        if expanded_command != *command {
                            let expanded_parts = parse_arguments(&expanded_command);
                            let mut final_args = expanded_parts.clone();
                            final_args.extend_from_slice(
                                &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                            );
                            let final_command = &final_args[0];
                            let final_arg_refs: Vec<&str> =
                                final_args[1..].iter().map(|s| s.as_str()).collect();
                            execute_command(final_command, &final_arg_refs);
                        } else {
                            execute_command(command, &args);
                        }
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

fn run_shell(
    history_file: PathBuf,
    prompt: Option<String>,
    file: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut signals = Signals::new([SIGINT])?;
    thread::spawn(move || for _sig in signals.forever() {});

    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();
    let helper = ShellHelper::new();
    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(helper));

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
    let file = matches.get_one::<PathBuf>("file").cloned();

    run_shell(history_file, prompt, file)
}
