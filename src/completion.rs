use colored::*;
use rustyline::completion::{Completer, Pair};
use rustyline::config::Config;
use rustyline::highlight::{CmdKind, Highlighter, MatchingBracketHighlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::{MatchingBracketValidator, Validator};
use rustyline::{CompletionType, Helper};
use rustyline::{Context, Editor};
use std::borrow::Cow;
use std::env;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub struct ShellHelper {
    completer: ShellCompleter,
    hinter: HistoryHinter,
    validator: MatchingBracketValidator,
    highlighter: MatchingBracketHighlighter,
}

impl Default for ShellHelper {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellHelper {
    pub fn new() -> ShellHelper {
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
            "set".to_string(),
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

        let expanded_path = if let Some(stripped) = partial_path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(stripped).to_string_lossy().to_string()
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
                        let is_dir = entry.file_type().is_ok_and(|ft| ft.is_dir());
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
                            if dir_path == Path::new("/") {
                                format!("/{}", name)
                            } else {
                                format!("{}/{}", dir_path.display(), name)
                            }
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

pub fn create_editor()
-> Result<Editor<ShellHelper, rustyline::history::FileHistory>, Box<dyn std::error::Error>> {
    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();
    let helper = ShellHelper::new();
    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(helper));
    Ok(rl)
}
