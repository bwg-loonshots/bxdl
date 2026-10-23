//! Advisory shell completion. This module never dispatches the typed command.
//! Only catalog aliases and bounded private namespace names are read from disk.
use super::{
    catalog::Catalog,
    home::{self, Home},
};
use std::{collections::BTreeMap, path::PathBuf};

const COMMANDS: &[&str] = &[
    "cli",
    "nodes",
    "setup",
    "init",
    "resume-init",
    "start",
    "stop",
    "status",
    "logs",
    "diagnose",
    "config",
    "preflight",
    "instance",
    "install",
    "package",
    "runtime",
    "engine",
    "completion",
    "version",
    "help",
    "console",
];

#[derive(Clone, Copy, PartialEq)]
enum Value {
    Flag,
    File,
    Directory,
    Field,
    None,
}
#[derive(Clone, Copy, PartialEq)]
enum Position {
    None,
    Node,
    Session,
    Preparation,
    NewName,
    File,
    Shell,
}
struct Spec {
    name: String,
    options: Vec<(&'static str, Value)>,
    position: Position,
    json: bool,
}
impl Spec {
    fn new(name: &str, position: Position, options: &[(&'static str, Value)]) -> Self {
        Self {
            name: name.into(),
            position,
            options: options.to_vec(),
            json: true,
        }
    }
    fn option(&self, name: &str) -> Option<Value> {
        self.options
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| *v)
    }
}

/// None denotes an invalid provider request, not an invalid partial command.
/// Keeping the protocol separate also protects a typed --json from CLI parsing.
pub(crate) fn output(args: &[String]) -> Option<String> {
    if args == ["zsh"] {
        return Some(include_str!("../../completions/_bxdl").into());
    }
    if args.len() < 5 || args[0] != "__complete" || args[1] != "--cursor" || args[3] != "--" {
        return None;
    }
    if args[2].is_empty() || !args[2].bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let cursor = args[2].parse::<usize>().ok()?;
    let words = &args[4..];
    if words.len() > 128
        || cursor >= words.len()
        || words.iter().any(|w| w.len() > 4096)
        || words.iter().map(String::len).sum::<usize>() > 16384
    {
        return None;
    }
    Some(complete(words, cursor).unwrap_or_else(|| "none\n".into()))
}

fn values<'a>(items: impl IntoIterator<Item = &'a str>, prefix: &str) -> String {
    let mut items: Vec<_> = items
        .into_iter()
        .filter(|item| item.starts_with(prefix))
        .collect();
    items.sort_unstable();
    items.dedup();
    let mut result = String::from("values\n");
    for item in items {
        result.push_str(item);
        result.push('\n');
    }
    result
}

fn subcommands(command: &str) -> &'static [&'static str] {
    match command {
        "cli" => &["install", "update", "info", "rollback", "bundle"],
        "cli bundle" => &["build", "verify"],
        "setup" => &["new", "resume", "list", "show", "edit", "apply"],
        "nodes" => &["add"],
        "config" => &["validate", "fingerprint"],
        "package" => &["build", "verify"],
        "runtime" => &["stage"],
        "engine" => &["inspect", "preflight"],
        "instance" => &["register", "show"],
        "completion" => &["zsh"],
        _ => &[],
    }
}

fn spec(command: &str) -> Option<Spec> {
    use Position::{File, NewName, Node, None, Preparation, Session, Shell};
    use Value::{Directory as D, Field, File as F, Flag as B, None as N};
    let mut s = match command {
        "cli install" | "cli update" => Spec::new(
            command,
            None,
            &[
                ("--root", D),
                ("--bin-dir", D),
                ("--from", F),
                ("--public-key", F),
                ("--allow-unsigned-development", B),
            ],
        ),
        "cli info" | "cli rollback" => Spec::new(command, None, &[("--root", D), ("--bin-dir", D)]),
        "cli bundle build" => Spec::new(
            command,
            None,
            &[
                ("--root", D),
                ("--output", F),
                ("--version", N),
                ("--revision", N),
                ("--signing-key", F),
                ("--allow-unsigned-development", B),
            ],
        ),
        "cli bundle verify" => Spec::new(
            command,
            File,
            &[("--public-key", F), ("--allow-unsigned-development", B)],
        ),
        "nodes" => Spec::new(command, None, &[("--home", D)]),
        "nodes add" => Spec::new(command, NewName, &[("--home", D), ("--instance", D)]),
        "setup new" => Spec::new(command, NewName, &[("--home", D), ("--from", F)]),
        "setup resume" | "setup apply" => Spec::new(command, Session, &[("--home", D)]),
        "setup edit" => Spec::new(command, Session, &[("--home", D), ("--field", Field)]),
        "setup list" => Spec::new(command, None, &[("--home", D)]),
        "setup show" => Spec::new(command, Preparation, &[("--home", D)]),
        "setup" => Spec::new(
            command,
            None,
            &[
                ("--workspace", D),
                ("--resume", B),
                ("--from", F),
                ("--output", F),
                ("--non-interactive", B),
                ("--install", B),
                ("--network", B),
                ("--network-file", F),
            ],
        ),
        "init" | "resume-init" | "start" | "stop" | "status" | "logs" | "diagnose" => {
            let mut s = Spec::new(
                command,
                Node,
                &[("--home", D), ("--instance", D), ("--timeout-seconds", N)],
            );
            match command {
                "init" => s.options.push(("--confirm-initialize", B)),
                "resume-init" => s.options.push(("--confirm-resume", B)),
                "status" => s.options.push(("--wait", B)),
                "logs" | "diagnose" => s.options.extend([("--tail", N), ("--max-bytes", N)]),
                _ => (),
            }
            if command == "diagnose" {
                s.options.push(("--output", F));
            }
            s
        }
        "install" => Spec::new(
            command,
            File,
            &[
                ("--destination", D),
                ("--public-key", F),
                ("--allow-unsigned-development", B),
            ],
        ),
        "package verify" => Spec::new(
            command,
            File,
            &[("--public-key", F), ("--allow-unsigned-development", B)],
        ),
        "package build" => Spec::new(
            command,
            None,
            &[
                ("--root", D),
                ("--spec", F),
                ("--output", F),
                ("--signing-key", F),
                ("--allow-unsigned-development", B),
            ],
        ),
        "runtime stage" => Spec::new(
            command,
            None,
            &[("--source", D), ("--lock", F), ("--output", D)],
        ),
        "config validate" => Spec::new(command, None, &[("--file", F)]),
        "config fingerprint" => Spec::new(command, None, &[("--certificate", F)]),
        "engine inspect" | "engine preflight" => {
            let mut s = Spec::new(
                command,
                None,
                &[
                    ("--jar", F),
                    ("--java", F),
                    ("--lock", F),
                    ("--allow-development", B),
                    ("--timeout-seconds", N),
                ],
            );
            if command == "engine preflight" {
                s.options.push(("--config", F));
            }
            s
        }
        "preflight" => Spec::new(
            command,
            None,
            &[
                ("--config", F),
                ("--engine-config", F),
                ("--jar", F),
                ("--java", F),
                ("--lock", F),
                ("--allow-development", B),
                ("--timeout-seconds", N),
                ("--instance", D),
            ],
        ),
        "instance show" => Spec::new(command, None, &[("--instance", D)]),
        "instance register" => Spec::new(
            command,
            None,
            &[
                ("--instance", D),
                ("--package", D),
                ("--archive", F),
                ("--public-key", F),
                ("--allow-unsigned-development", B),
                ("--config", F),
                ("--engine-config", F),
                ("--lock", F),
                ("--allow-development", B),
                ("--timeout-seconds", N),
            ],
        ),
        "console" => Spec::new(command, None, &[("--home", D), ("--instance", D)]),
        "completion" => Spec::new(command, Shell, &[]),
        "version" => Spec::new(command, None, &[]),
        _ => return Option::None,
    };
    s.json = !matches!(
        command,
        "setup new" | "setup resume" | "setup edit" | "setup apply" | "console" | "completion"
    );
    Some(s)
}

// Constraints are partial-command constraints: missing required inputs are fine.
// Impossible combinations fail closed, without inspecting paths or executing CLI validation.
fn compatible(spec: &Spec, used: &BTreeMap<&str, &str>, positions: usize) -> bool {
    let has = |flag| used.contains_key(flag);
    if (has("--json") && !spec.json)
        || (has("--allow-unsigned-development") && (has("--public-key") || has("--signing-key")))
    {
        return false;
    }
    if spec.position == Position::Node && has("--instance") && (positions > 0 || has("--home")) {
        return false;
    }
    if spec.name == "preflight"
        && has("--instance")
        && used
            .keys()
            .any(|key| !matches!(*key, "--instance" | "--timeout-seconds" | "--json"))
    {
        return false;
    }
    if spec.name == "setup" {
        if has("--resume") && has("--from") {
            return false;
        }
        if has("--install")
            && [
                "--network",
                "--network-file",
                "--output",
                "--non-interactive",
                "--json",
            ]
            .iter()
            .any(|f| has(*f))
        {
            return false;
        }
        if has("--network")
            && (has("--non-interactive")
                || has("--json")
                || (has("--resume") && has("--network-file")))
        {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy)]
enum Context<'a> {
    Options(&'a str),
    Position(&'a str),
    Value(Value, &'a str),
}

fn complete(words: &[String], original_cursor: usize) -> Option<String> {
    // Match CLI's global JSON extraction, retaining indices around the cursor.
    let mut json = false;
    let mut literal = false;
    let mut cursor = 0;
    let mut line = Vec::new();
    for (index, word) in words.iter().enumerate() {
        if word == "--json" && !literal && index != original_cursor {
            json = true;
            continue;
        }
        if index == original_cursor {
            cursor = line.len();
        }
        if word == "--" && index != original_cursor {
            literal = true;
        }
        line.push(word.as_str());
    }
    let current = line[cursor];
    if cursor == 0 {
        return Some(if current.starts_with('-') {
            values(
                ["--help", "-h", "--json"]
                    .into_iter()
                    .filter(|s| *s != "--json" || !json),
                current,
            )
        } else {
            values(COMMANDS.iter().copied(), current)
        });
    }
    let mut command = line[0].to_owned();
    if command == "help" {
        return if cursor == 1 {
            Some(values(
                COMMANDS.iter().copied().filter(|s| *s != "help"),
                current,
            ))
        } else {
            let topic = line[1..cursor].join(" ");
            if spec(&topic).is_some() || !subcommands(&topic).is_empty() {
                Some(values(subcommands(&topic).iter().copied(), current))
            } else {
                None
            }
        };
    }
    if !COMMANDS.contains(&command.as_str()) {
        return None;
    }
    let mut start = 1;
    loop {
        let subs = subcommands(&command);
        if subs.is_empty() || command == "completion" {
            break;
        }
        if cursor == start && !current.starts_with('-') {
            // Validate suffix options for each candidate without reading names.
            let candidates = subs.iter().copied().filter(|sub| {
                let mut candidate_line = line.clone();
                candidate_line[start] = sub;
                command_syntax(
                    &format!("{command} {sub}"),
                    &candidate_line,
                    start + 1,
                    json,
                )
            });
            return Some(values(candidates, current));
        }
        if line.len() > start && cursor != start && subs.contains(&line[start]) {
            command.push(' ');
            command.push_str(line[start]);
            start += 1;
        } else if matches!(command.as_str(), "setup" | "nodes") {
            break;
        } else {
            return if cursor == start {
                Some(values(help_options(&line, cursor), current))
            } else {
                None
            };
        }
    }
    let spec = spec(&command)?;
    arguments(&spec, &line, start, cursor, json, false)
}

/// Nested command candidates inspect syntax only. No filesystem lookup or
/// dispatch is performed while deciding between subcommands.
fn command_syntax(command: &str, line: &[&str], start: usize, json: bool) -> bool {
    if let Some(spec) = spec(command) {
        let mut candidate_line = line.to_vec();
        candidate_line.push("");
        return arguments(
            &spec,
            &candidate_line,
            start,
            candidate_line.len() - 1,
            json,
            true,
        )
        .is_some();
    }
    let subs = subcommands(command);
    if subs.is_empty() {
        return false;
    }
    let Some(next) = line.get(start) else {
        return true;
    };
    if subs.contains(next) {
        command_syntax(&format!("{command} {next}"), line, start + 1, json)
    } else {
        let help_line: Vec<String> = line.iter().map(|word| (*word).into()).collect();
        super::help::for_args(&help_line).is_some()
    }
}

fn arguments(
    spec: &Spec,
    line: &[&str],
    start: usize,
    cursor: usize,
    json: bool,
    syntax_only: bool,
) -> Option<String> {
    let mut used = BTreeMap::new();
    if json {
        used.insert("--json", "");
    }
    let mut positional = 0;
    let mut options = true;
    let mut pending = None;
    let mut context = None;
    let mut position_options = false;
    for (index, word) in line.iter().copied().enumerate().skip(start) {
        if let Some((name, value)) = pending.take() {
            if index != cursor && (word.is_empty() || word.starts_with("--")) {
                return None;
            }
            used.insert(name, word);
            if index == cursor {
                context = Some(Context::Value(value, word));
            }
            continue;
        }
        if index == cursor {
            if options && word.starts_with('-') {
                if let Some((name, prefix)) = word.split_once('=') {
                    let value = spec.option(name)?;
                    if value == Value::Flag || used.insert(name, prefix).is_some() {
                        return None;
                    }
                    context = Some(Context::Value(value, prefix));
                } else {
                    context = Some(Context::Options(word));
                }
            } else if spec.position == Position::None || positional > 0 {
                if !word.is_empty() {
                    return None;
                }
                context = if options {
                    Some(Context::Options(word))
                } else {
                    None
                };
            } else {
                positional += 1;
                context = Some(Context::Position(word));
                position_options = options;
            }
            continue;
        }
        if options && word == "--" {
            options = false;
            continue;
        }
        if !options || !word.starts_with('-') {
            positional += 1;
            if spec.position == Position::None || positional > 1 {
                return None;
            }
            continue;
        }
        let (name, inline) = word
            .split_once('=')
            .map_or((word, None), |(n, v)| (n, Some(v)));
        let value = spec.option(name)?;
        if used.insert(name, inline.unwrap_or("")).is_some() {
            return None;
        }
        if value == Value::Flag {
            if inline.is_some() {
                return None;
            }
        } else if inline.is_none() {
            pending = Some((name, value));
        } else if inline == Some("") {
            return None;
        }
    }
    if spec.position == Position::Node
        && used.contains_key("--instance")
        && position_options
        && matches!(context, Some(Context::Position("")))
    {
        positional -= 1;
        context = Some(Context::Options(""));
    }
    if pending.is_some() || !compatible(spec, &used, positional) {
        return None;
    }
    if syntax_only {
        return Some(String::new());
    }
    match context? {
        Context::Options(prefix) => {
            let mut names = help_options(line, cursor);
            if spec.json {
                names.push("--json");
            }
            names.extend(spec.options.iter().map(|(name, _)| *name));
            names.retain(|name| {
                if used.contains_key(name) {
                    return false;
                }
                let mut trial = used.clone();
                trial.insert(name, "");
                compatible(spec, &trial, positional)
            });
            Some(values(names, prefix))
        }
        Context::Value(kind, prefix) => Some(match kind {
            Value::File => "files\n".into(),
            Value::Directory => "directories\n".into(),
            Value::Field => values(crate::setup::FIELDS.iter().map(|f| f.name), prefix),
            _ => "none\n".into(),
        }),
        Context::Position(prefix) => Some(match spec.position {
            Position::Shell => values(["zsh"], prefix),
            Position::File => "files\n".into(),
            Position::Node | Position::Session | Position::Preparation => {
                let names = names(spec.position, used.get("--home").copied()).unwrap_or_default();
                values(names.iter().map(String::as_str), prefix)
            }
            _ => "none\n".into(),
        }),
    }
}

fn help_options(line: &[&str], cursor: usize) -> Vec<&'static str> {
    let mut help_line: Vec<String> = line.iter().map(|word| (*word).into()).collect();
    help_line[cursor] = "--help".into();
    if super::help::for_args(&help_line).is_some() {
        vec!["--help", "-h"]
    } else {
        Vec::new()
    }
}

fn names(kind: Position, explicit_home: Option<&str>) -> Option<Vec<String>> {
    let path = match explicit_home {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        Some(_) => return None,
        None => home::default_home().ok()?,
    };
    if kind == Position::Node {
        let catalog = Catalog::open(&path).ok()??;
        return Some(
            catalog
                .entries()
                .iter()
                .filter_map(|entry| entry.alias.clone())
                .collect(),
        );
    }
    Home::open(&path)
        .ok()??
        .completion_names(kind == Position::Preparation)
        .ok()
}
