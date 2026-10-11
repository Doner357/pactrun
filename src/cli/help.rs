//! Static help queries share one catalog across Human, JSON and JSONL.
use super::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt::Write as _, sync::OnceLock};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    description: String,
    global_options: Vec<HelpOption>,
    options: BTreeMap<String, HelpOption>,
    commands: Vec<Entry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: Vec<String>,
    description: String,
    forms: Vec<Form>,
    options: Vec<String>,
    notes: Vec<String>,
    examples: Vec<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct HelpOption {
    name: String,
    value: Option<String>,
    description: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Form {
    usage: String,
    description: String,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct HelpCommand {
    path: Vec<String>,
    description: String,
    forms: Vec<Form>,
    options: Vec<HelpOption>,
    notes: Vec<String>,
    examples: Vec<String>,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Help {
    pub(super) usage: String,
    pub(super) scope: Vec<String>,
    command: Option<HelpCommand>,
    commands: Vec<HelpCommand>,
    global_options: Vec<HelpOption>,
}

pub(super) struct Request(Vec<String>);
pub(super) fn root() -> Request {
    Request(Vec::new())
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("help_catalog.json"))
            .expect("checked static help catalog")
    })
}

pub(super) fn is_query(args: &[OsString]) -> bool {
    if args.is_empty() {
        return true;
    }
    let helper = args.first().is_some_and(|arg| arg == "hook");
    args.iter()
        .position(|arg| arg == "--help" || (helper && arg == "-h"))
        .is_some_and(|at| {
            args[..at]
                .iter()
                .all(|arg| arg.to_str().is_some_and(|word| !word.starts_with('-')))
        })
}

pub(super) fn request(args: &[OsString]) -> Option<Request> {
    if args.is_empty() {
        return Some(root());
    }
    let (last, prefix) = args.split_last()?;
    if last != "--help" && !(last == "-h" && prefix.first().is_some_and(|p| p == "hook")) {
        return None;
    }
    let path = prefix
        .iter()
        .map(|p| p.to_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;
    // Help paths contain command words, not operands or option values. In
    // particular, a parameter whose literal value is --help is not intercepted.
    (path.is_empty() || catalog().commands.iter().any(|c| c.path == path)).then_some(Request(path))
}

fn project(entry: &Entry) -> HelpCommand {
    HelpCommand {
        path: entry.path.clone(),
        description: entry.description.clone(),
        forms: entry.forms.clone(),
        options: entry
            .options
            .iter()
            .map(|name| {
                catalog()
                    .options
                    .get(name)
                    .expect("checked help option reference")
                    .clone()
            })
            .collect(),
        notes: entry.notes.clone(),
        examples: entry.examples.clone(),
    }
}

impl Request {
    pub(super) fn result(self) -> Help {
        let scope = self.0;
        let command = catalog()
            .commands
            .iter()
            .find(|c| c.path == scope)
            .map(project);
        let commands = catalog()
            .commands
            .iter()
            .filter(|c| c.path.starts_with(&scope) && c.path.len() > scope.len())
            .map(project)
            .collect();
        let global_options = catalog()
            .global_options
            .iter()
            .filter(|o| scope.is_empty() || o.name != "--version")
            .cloned()
            .collect();
        let mut help = Help {
            usage: String::new(),
            scope,
            command,
            commands,
            global_options,
        };
        help.usage = render(&help);
        help
    }
}

fn option_rows(text: &mut String, options: &[HelpOption]) {
    for option in options {
        let syntax = match &option.value {
            Some(value) => format!("{} <{value}>", option.name),
            None => option.name.clone(),
        };
        let _ = writeln!(text, "  {syntax}\n    {}", option.description);
    }
}

fn render(help: &Help) -> String {
    let mut text = format!("Pactrun {}\nUsage:\n", env!("CARGO_PKG_VERSION"));
    let path = if help.scope.is_empty() {
        "pactrun".into()
    } else {
        format!("pactrun {}", help.scope.join(" "))
    };
    let forms = help
        .command
        .as_ref()
        .map(|c| c.forms.as_slice())
        .unwrap_or(&[]);
    if forms.is_empty() {
        let _ = writeln!(
            text,
            "  {path} {}<command> [options]",
            if help.scope.is_empty() {
                "[--format human|json|jsonl] "
            } else {
                ""
            }
        );
        let _ = writeln!(
            text,
            "\n{}",
            help.command
                .as_ref()
                .map(|c| c.description.as_str())
                .unwrap_or(&catalog().description)
        );
    } else {
        for form in forms {
            let _ = writeln!(text, "  {}\n    {}", form.usage, form.description);
        }
    }
    let children: Vec<_> = help
        .commands
        .iter()
        .filter(|c| c.path.len() == help.scope.len() + 1)
        .collect();
    if !children.is_empty() {
        text.push_str("\nCommands\n");
        let width = children
            .iter()
            .map(|c| c.path.last().unwrap().len())
            .max()
            .unwrap();
        for child in children {
            let _ = writeln!(
                text,
                "  {:width$}  {}",
                child.path.last().unwrap(),
                child.description
            );
        }
    }
    if let Some(command) = &help.command {
        if !command.options.is_empty() {
            text.push_str("\nOptions\n");
            option_rows(&mut text, &command.options);
        }
        if !command.notes.is_empty() {
            text.push_str("\nNotes\n");
            for note in &command.notes {
                let _ = writeln!(text, "  {note}");
            }
        }
        if !command.examples.is_empty() {
            text.push_str("\nExamples\n");
            for example in &command.examples {
                let _ = writeln!(text, "  {example}");
            }
        }
    }
    text.push_str("\nGlobal options\n");
    option_rows(&mut text, &help.global_options);
    if !help.commands.is_empty() {
        let _ = writeln!(text, "\nRun '{path} <command> --help' for details.");
    }
    text
}

#[cfg(test)]
#[path = "help_tests.rs"]
mod tests;
