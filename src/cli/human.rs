//! Human view of the explicit public result projections. No storage or execution.
//! The view tree is created in memory from an owned typed result, never parsed
//! from stdout, encoded JSON, Hook text, or a second application query.
use super::*;
use serde_json::Value;

fn write(out: &mut dyn Write, text: impl AsRef<str>) -> Result<(), CliError> {
    writeln!(out, "{}", text.as_ref()).map_err(presentation::output_error)
}
fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .map(scalar)
        .unwrap_or_else(|| "Not provided".into())
}
fn scalar(value: &Value) -> String {
    match value {
        Value::Null => "Not provided".into(),
        Value::Bool(v) => if *v { "Yes" } else { "No" }.into(),
        Value::String(s) => catalog::safe(s),
        Value::Number(n) => n.to_string(),
        Value::Array(a) if a.is_empty() => "None".into(),
        Value::Array(a) => format!("{} entries", a.len()),
        Value::Object(_) => String::new(),
    }
}
pub(super) fn quoted(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization")
}
fn command_argument(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-.".contains(&c))
    {
        return value.to_owned();
    }
    #[cfg(windows)]
    let escaped = value
        .replace('\'', "''")
        .replace('\u{2018}', "\u{2018}\u{2018}")
        .replace('\u{2019}', "\u{2019}\u{2019}");
    #[cfg(not(windows))]
    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}
pub(super) fn path_text(value: &Value) -> String {
    match value["encoding"].as_str() {
        Some("utf8") => value["value"]
            .as_str()
            .map(quoted)
            .unwrap_or_else(|| "Not provided".into()),
        Some("windows_wide") => format!(
            "UTF-16 units [{}]",
            value["units"]
                .as_array()
                .map(|a| a
                    .iter()
                    .filter_map(Value::as_u64)
                    .map(|v| format!("{v:04x}"))
                    .collect::<Vec<_>>()
                    .join(" "))
                .unwrap_or_default()
        ),
        Some("unix_bytes") => format!(
            "Unix bytes [{}]",
            value["bytes"]
                .as_array()
                .map(|a| a
                    .iter()
                    .filter_map(Value::as_u64)
                    .map(|v| format!("{v:02x}"))
                    .collect::<Vec<_>>()
                    .join(" "))
                .unwrap_or_default()
        ),
        _ => "Not provided".into(),
    }
}
fn label(key: &str) -> String {
    key.split('_')
        .map(|part| match part {
            "id" => "ID".into(),
            "ids" => "IDs".into(),
            "utc" => "UTC".into(),
            _ => {
                let mut chars = part.chars();
                chars
                    .next()
                    .map(|c| c.to_uppercase().to_string() + chars.as_str())
                    .unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn tree(value: &Value, out: &mut dyn Write, indent: usize) -> Result<(), CliError> {
    let pad = " ".repeat(indent);
    match value {
        Value::Object(fields) if fields.contains_key("encoding") => {
            write(out, format!("{pad}{}", path_text(value)))?;
        }
        Value::Object(fields) => {
            for (key, value) in fields {
                if matches!(
                    key.as_str(),
                    "related_revisions"
                        | "metadata_scope"
                        | "hook_args_count"
                        | "unique_prefix_length"
                        | "run_prefix_length"
                        | "instance_prefix_length"
                ) {
                    continue;
                }
                if matches!(value,Value::Array(values) if values.is_empty()) {
                    continue;
                }
                if key == "metadata" && value.is_object() {
                    author_text(value, out, indent)?;
                    continue;
                }
                if key == "summary" {
                    author_field(key, value, out, indent)?;
                    continue;
                }
                if (matches!(
                    key.as_str(),
                    "display_name" | "summary" | "description" | "help"
                ) && value.is_null())
                    || (key == "metadata"
                        && value
                            .as_object()
                            .is_some_and(|m| m.values().all(Value::is_null)))
                {
                    continue;
                }
                let heading = label(key);
                if value.is_object() || value.is_array() {
                    write(out, format!("{pad}{heading}"))?;
                    tree(value, out, indent + 2)?;
                } else {
                    write(out, format!("{pad}{heading}: {}", scalar(value)))?;
                }
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                if value.is_object() || value.is_array() {
                    write(out, format!("{pad}{}.", index + 1))?;
                    tree(value, out, indent + 2)?;
                } else {
                    write(out, format!("{pad}- {}", scalar(value)))?;
                }
            }
        }
        _ => write(out, format!("{pad}{}", scalar(value)))?,
    }
    Ok(())
}
fn table(headers: &[&str], rows: Vec<Vec<String>>, out: &mut dyn Write) -> Result<(), CliError> {
    let widths: Vec<_> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            rows.iter()
                .filter_map(|r| r.get(i))
                .map(|s| s.chars().count())
                .max()
                .unwrap_or(0)
                .max(h.len())
        })
        .collect();
    for row in
        std::iter::once(headers.iter().map(|s| s.to_string()).collect::<Vec<_>>()).chain(rows)
    {
        let line = row
            .iter()
            .enumerate()
            .map(|(i, s)| {
                format!(
                    "{s}{}",
                    " ".repeat(widths[i].saturating_sub(s.chars().count()))
                )
            })
            .collect::<Vec<_>>()
            .join("  ");
        write(out, line.trim_end())?;
    }
    Ok(())
}
fn installation_time(value: &Value) -> String {
    catalog_presentation::installation_time(
        value
            .pointer("/local/installed_at_unix_ms")
            .and_then(Value::as_str)
            .and_then(|s| s.parse().ok()),
    )
}
fn display_id(value: &Value, key: &str, width: &str, display: &reply::DisplayOptions) -> String {
    let id = text(value, key);
    if !display.no_trunc
        && id.bytes().all(|b| b.is_ascii_hexdigit())
        && let Some(width) = value.get(width).and_then(Value::as_u64)
    {
        let end = (width as usize).max(12).min(id.len());
        return id[..end].into();
    }
    id
}
fn captured_time(value: &Value) -> String {
    let nanos = value
        .get("unix_seconds")
        .and_then(Value::as_str)
        .and_then(|v| v.parse::<i128>().ok())
        .and_then(|s| s.checked_mul(1_000_000_000))
        .zip(value.get("nanoseconds").and_then(Value::as_u64))
        .and_then(|(s, n)| s.checked_add(i128::from(n)));
    nanos
        .and_then(|n| time::OffsetDateTime::from_unix_timestamp_nanos(n).ok())
        .and_then(|v| {
            v.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_else(|| "Not provided".into())
}
fn operation(value: &Value) -> String {
    let kind = text(value, "kind");
    if let Some(action) = value.get("action_id").and_then(Value::as_str) {
        format!("{} {}", label(&kind), catalog::safe(action))
    } else {
        label(&kind)
    }
}
fn run_summary(value: &Value, out: &mut dyn Write, details: bool) -> Result<(), CliError> {
    let run = value.get("run").unwrap_or(value);
    write(
        out,
        format!(
            "Run: {}\nInstance: {}\nOperation: {}",
            text(run, "run_id"),
            text(run, "instance_id"),
            operation(&run["operation"])
        ),
    )?;
    let state = &run["state"];
    write(
        out,
        format!(
            "Outcome: {}",
            state
                .get("outcome")
                .map(scalar)
                .unwrap_or_else(|| text(state, "phase"))
        ),
    )?;
    for key in ["primary_failure", "secondary_failures"] {
        if let Some(f) = state
            .get(key)
            .filter(|v| !v.is_null() && !matches!(v,Value::Array(a) if a.is_empty()))
        {
            write(out, label(key))?;
            if key == "primary_failure" && f["cause"]["kind"] == "missing_required_inputs" {
                write(
                    out,
                    format!(
                        "  Missing required Inputs: {}",
                        f["cause"]["input_ids"]
                            .as_array()
                            .unwrap_or(&Vec::new())
                            .iter()
                            .map(scalar)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )?;
                write(out, format!("  Stage: {}", text(f, "step")))?;
                if !details {
                    write(out, "  Bind the missing Inputs before invoking again.")?;
                }
            } else {
                tree(f, out, 2)?;
            }
        }
    }
    if let Some(snapshot) = value.get("capture_result").filter(|v| !v.is_null()) {
        write(out, format!("Snapshot: {}", scalar(snapshot)))?;
    }
    for key in ["current_recovery_guard", "migration_progress"] {
        if let Some(v) = value.get(key).filter(|v| !v.is_null()) {
            write(out, label(key))?;
            tree(v, out, 2)?;
            if key == "current_recovery_guard" {
                write(
                    out,
                    format!("Guard details:\n  pactrun run show {}", text(v, "run_id")),
                )?;
            }
        }
    }
    if details || state["recovery_risk"] == "open" {
        write(
            out,
            format!("Recovery risk: {}", text(state, "recovery_risk")),
        )?;
    }
    if (details || state["outcome"] != "succeeded")
        && let Some(status) = state.get("hook_completion_status").filter(|v| !v.is_null())
    {
        write(out, format!("Hook completion: {}", scalar(status)))?;
    }
    if details {
        write(out, format!("Phase: {}", text(state, "phase")))?;
        if let Some(artifacts) = state["artifacts"].as_array() {
            for artifact in artifacts {
                write(
                    out,
                    format!(
                        "Artifact: {} ({} bytes)",
                        text(artifact, "output_id"),
                        text(artifact, "byte_length")
                    ),
                )?;
            }
        }
        if value["current_recovery_guard"].is_null() {
            write(out, "Current recovery guard: None")?;
        }
        for key in ["accepted_state_version", "accepted_at_unix_ms", "revision"] {
            if let Some(v) = run.get(key) {
                write(out, label(key))?;
                tree(v, out, 2)?;
            }
        }
        if let Some(v) = value.get("diagnostics").filter(|v| !v.is_null()) {
            diagnostic_details(v, out)?;
        }
    } else if state
        .get("outcome")
        .and_then(Value::as_str)
        .is_some_and(|o| o != "succeeded")
    {
        write(
            out,
            format!(
                "Inspect details:\n  pactrun run show {}",
                text(run, "run_id")
            ),
        )?;
    }
    Ok(())
}
fn diagnostic_details(value: &Value, out: &mut dyn Write) -> Result<(), CliError> {
    for (source, collection) in [("Hook", value), ("Pactrun", &value["pactrun"])] {
        if collection.is_null() {
            continue;
        }
        let events = collection["events"].as_array();
        if source == "Hook" && collection["retain_text"] == false {
            write(out, "Hook diagnostic text: not retained")?;
        }
        if let Some(events) = events {
            for event in events {
                let summary = event["message"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(catalog::safe)
                    .unwrap_or_else(|| {
                        if event["kind"] == "completion" {
                            format!("completion: {}", text(event, "completion_status"))
                        } else {
                            text(event, "kind")
                        }
                    });
                let code = event["code"]
                    .as_str()
                    .map(|v| format!(" [{}]", catalog::safe(v)))
                    .unwrap_or_default();
                let severity = event["severity"]
                    .as_str()
                    .map(|v| format!(" {}", catalog::safe(v)))
                    .unwrap_or_default();
                write(
                    out,
                    format!(
                        "{source} #{}{}{}: {summary}",
                        text(event, "sequence"),
                        severity,
                        code
                    ),
                )?;
                if let Some(stage) = event["stage"].as_str() {
                    write(out, format!("  Context: {}", catalog::safe(stage)))?;
                }
                if event["truncated"] == true {
                    write(out, "  Message: middle omitted")?;
                }
            }
            if let Some(observed) = collection["observed"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
            {
                let omitted = observed.saturating_sub(events.len() as u64);
                if omitted > 0 && (source != "Hook" || collection["retain_text"] != false) {
                    write(
                        out,
                        format!("{source}: {omitted} diagnostic events not retained"),
                    )?;
                }
            }
        }
        if collection["persistence_failed"] == true {
            write(out, format!("{source}: diagnostic storage failed"))?;
        }
        if collection["collection_closed"] == false && collection["started"] == true {
            write(
                out,
                format!("{source}: diagnostic collection did not finish"),
            )?;
        }
    }
    Ok(())
}
fn author_value(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| !s.is_empty())
}
fn summary_cell(metadata: &Value) -> String {
    author_value(&metadata["summary"])
        .map(|s| s.lines().map(catalog::safe).collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}
fn author_field(
    field: &str,
    value: &Value,
    out: &mut dyn Write,
    indent: usize,
) -> Result<(), CliError> {
    let pad = " ".repeat(indent);
    if let Some(s) = author_value(value) {
        write(out, format!("\n{pad}{}", label(field)))?;
        for line in s.lines() {
            write(out, format!("{pad}  {}", catalog::safe(line)))?;
        }
    }
    Ok(())
}
fn author_text(value: &Value, out: &mut dyn Write, indent: usize) -> Result<(), CliError> {
    for field in ["display_name", "summary", "description", "help"] {
        author_field(field, &value[field], out, indent)?;
    }
    Ok(())
}
fn input_summary(value: &Value, item: &Value) -> String {
    // Input list supplies exactly the Instance's active defining Revision.
    // A retained binding has no declaration in that Revision to borrow text from.
    if item["role"] != "active" {
        return String::new();
    }
    let Some(revisions) = value["related_revisions"].as_array() else {
        return String::new();
    };
    let [revision] = revisions.as_slice() else {
        return String::new();
    };
    revision
        .pointer("/declarations/capabilities/inputs")
        .and_then(Value::as_array)
        .and_then(|inputs| {
            inputs
                .iter()
                .find(|input| input["input_id"] == item["input_id"])
        })
        .map(|input| summary_cell(&input["metadata"]))
        .unwrap_or_default()
}
fn migration_summary(value: &Value, source: &Value, target: &Value) -> String {
    value["presentation"]
        .as_array()
        .and_then(|groups| groups.iter().find(|group| group["revision"] == *target))
        .and_then(|group| group["entries"].as_array())
        .and_then(|entries| {
            entries.iter().find(|entry| {
                entry["target"]["kind"] == "migration_edge"
                    && entry["target"]["source_digest"] == source["content_digest"]
            })
        })
        .map(summary_cell)
        .unwrap_or_default()
}
fn page_footer(
    value: &Value,
    display: &reply::DisplayOptions,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    if let Some(items) = value.get("items").and_then(Value::as_array) {
        write(
            out,
            format!(
                "\n{} record{} shown.",
                items.len(),
                if items.len() == 1 { "" } else { "s" }
            ),
        )?;
    }
    if let Some(cursor) = value
        .get("next")
        .and_then(Value::as_str)
        .and_then(|c| display.continuation(c))
    {
        write(
            out,
            format!("More results available. Continue:\n  {cursor}"),
        )?;
    }
    Ok(())
}
fn instance(value: &Value, out: &mut dyn Write) -> Result<(), CliError> {
    write(out, format!("Instance: {}", text(value, "name")))?;
    write(
        out,
        format!(
            "\nRequired inputs: {}",
            if value["required_inputs_satisfied"].as_bool() == Some(true) {
                "Satisfied"
            } else {
                "Missing"
            }
        ),
    )?;
    write(
        out,
        format!(
            "Recovery: {}",
            if value.get("recovery_guard").is_none_or(Value::is_null) {
                "None"
            } else {
                "Required"
            }
        ),
    )?;
    if let Some(inputs) = value
        .get("inputs")
        .filter(|v| v.as_array().is_some_and(|a| !a.is_empty()))
    {
        write(out, "\nInputs")?;
        let inputs = inputs.as_array().expect("checked Input array");
        table(
            &["INPUT", "ROLE", "REQUIRED", "STATUS", "PROTECTION"],
            inputs
                .iter()
                .map(|input| {
                    vec![
                        text(input, "input_id"),
                        text(input, "role"),
                        if input["required"] == true {
                            "Yes"
                        } else {
                            "No"
                        }
                        .into(),
                        if input["present"] == true {
                            "Present"
                        } else {
                            "Not set"
                        }
                        .into(),
                        text(input, "protection"),
                    ]
                })
                .collect(),
            out,
        )?;
        let missing: Vec<_> = inputs
            .iter()
            .filter(|input| input["required"] == true && input["present"] == false)
            .collect();
        if !missing.is_empty() {
            write(
                out,
                "\nNext: bind the required Inputs before running an Action.",
            )?;
            for input in missing {
                write(
                    out,
                    format!(
                        "  pactrun input set {} {} --file <path>",
                        command_argument(value["name"].as_str().unwrap_or_default()),
                        text(input, "input_id")
                    ),
                )?;
            }
        }
    }
    write(out, "\nIdentity")?;
    write(
        out,
        format!("  Instance ID: {}", text(value, "instance_id")),
    )?;
    if let Some(revision) = value.get("active_revision") {
        write(
            out,
            format!(
                "  Revision: {}:{}",
                text(revision, "package_id"),
                text(revision, "content_digest").trim_start_matches("sha256:")
            ),
        )?;
    }
    write(
        out,
        format!("  State version: {}", text(value, "state_version")),
    )
}

pub(super) fn partial(value: &Value, out: &mut dyn Write) -> Result<(), CliError> {
    write(out, "Partial result")?;
    if value.get("run").is_some() {
        return run_summary(value, out, false);
    }
    tree(value, out, 2)
}

pub(super) fn render(
    command: &str,
    value: &Value,
    display: &reply::DisplayOptions,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    match command {
        "help" => {
            out.write_all(value["usage"].as_str().unwrap_or_default().as_bytes())
                .map_err(presentation::output_error)?;
            return Ok(());
        }
        "version" => return write(out, format!("pactrun {}", text(value, "product_version"))),
        "pack generate-id" => return write(out, text(value, "package_id")),
        "pack install" => {
            write(
                out,
                if value["newly_installed"].as_bool() == Some(true) {
                    "Pack installed."
                } else {
                    "Revision already installed."
                },
            )?;
            write(
                out,
                format!(
                    "\nRevision: {}\nInstalled: {}",
                    text(value, "reference"),
                    installation_time(value)
                ),
            )?;
            for key in ["kept_metadata", "migrations"] {
                if let Some(values) = value
                    .get(key)
                    .filter(|v| v.as_array().is_some_and(|a| !a.is_empty()))
                {
                    write(
                        out,
                        if key == "kept_metadata" {
                            "\nMetadata changes not applied (kept existing values).".into()
                        } else {
                            format!("\n{}", label(key))
                        },
                    )?;
                    tree(values, out, 2)?;
                }
            }
            return Ok(());
        }
        "run artifact export" => return write(out, "Artifact exported"),
        "run artifact delete" => {
            return write(
                out,
                match value["outcome"].as_str() {
                    Some("deleted") => "Artifact deleted.",
                    Some("already_absent") => "Artifact already absent.",
                    _ => return Err(CliError::operation("invalid Artifact deletion result")),
                },
            );
        }
        "revision export" | "snapshot export" => {
            write(
                out,
                if command == "revision export" {
                    "Pack exported."
                } else {
                    "Snapshot exported."
                },
            )?;
            return write(out, format!("Output: {}", path_text(&value["output"])));
        }
        "revision note show" | "revision trust show" => {
            let key = if command == "revision note show" {
                "note"
            } else {
                "trust"
            };
            return write(
                out,
                format!(
                    "{}: {}",
                    label(key),
                    value[key]
                        .as_str()
                        .map(quoted)
                        .unwrap_or_else(|| "Not set".into())
                ),
            );
        }
        "instance history show" => {
            write(out, "Instance history")?;
            tree(value, out, 2)?;
            return write(
                out,
                format!(
                    "\nRuns: pactrun run list --instance-id {}",
                    text(value, "instance_id")
                ),
            );
        }
        "instance create" | "instance show" => {
            let created = value.get("created_instance").unwrap_or(value);
            if command == "instance create" {
                write(out, "Instance created.\n")?;
            }
            instance(created, out)?;
            if let Some(restore) = value.get("restore").filter(|v| !v.is_null()) {
                write(out, "\nRestore")?;
                tree(restore, out, 2)?;
            }
            return Ok(());
        }
        "package rename" | "revision rename" => {
            return write(out, format!("Name set: {}", text(value, "name")));
        }
        "package unname" | "revision unname" => return write(out, "Name removed."),
        "input set" | "input delete" => {
            write(
                out,
                format!(
                    "Input {}: {}",
                    if command == "input set" {
                        "set"
                    } else {
                        "deleted"
                    },
                    text(value, "input_id")
                ),
            )?;
            write(
                out,
                format!(
                    "Instance ID: {}\nState version: {}",
                    text(value, "instance_id"),
                    text(value, "state_version")
                ),
            )?;
            return Ok(());
        }
        "instance list" => {
            let items = value["items"]
                .as_array()
                .ok_or_else(|| CliError::operation("invalid Instance list result"))?;
            if items.is_empty() {
                return write(out, "No instances yet.");
            }
            return table(
                &["NAME", "REQUIRED INPUTS", "RECOVERY"],
                items
                    .iter()
                    .map(|item| {
                        vec![
                            text(item, "name"),
                            if item["required_inputs_satisfied"].as_bool() == Some(true) {
                                "Satisfied"
                            } else {
                                "Missing"
                            }
                            .into(),
                            if item["recovery_guard"].is_null() {
                                "None"
                            } else {
                                "Required"
                            }
                            .into(),
                        ]
                    })
                    .collect(),
                out,
            );
        }
        "input list" => {
            let items = value["items"]
                .as_array()
                .ok_or_else(|| CliError::operation("invalid Input list result"))?;
            if items.is_empty() {
                return write(out, "No Inputs.");
            }
            return table(
                &["INPUT", "ROLE", "STATUS", "PROTECTION", "SUMMARY"],
                items
                    .iter()
                    .map(|item| {
                        vec![
                            text(item, "input_id"),
                            match item["required"].as_bool() {
                                Some(true) => "Active (required)",
                                Some(false) => "Active (optional)",
                                None => "Retained",
                            }
                            .into(),
                            if item["present"].as_bool() == Some(true) {
                                "Present"
                            } else {
                                "Absent"
                            }
                            .into(),
                            text(item, "protection"),
                            input_summary(value, item),
                        ]
                    })
                    .collect(),
                out,
            );
        }
        "action list" => {
            let items = value["items"]
                .as_array()
                .ok_or_else(|| CliError::operation("invalid Action list result"))?;
            if items.is_empty() {
                return write(out, "No Actions declared.");
            }
            return table(
                &["ACTION", "ACCESS", "SUMMARY"],
                items
                    .iter()
                    .map(|item| {
                        vec![
                            text(item, "action_id"),
                            text(item, "access"),
                            summary_cell(&item["metadata"]),
                        ]
                    })
                    .collect(),
                out,
            );
        }
        "action show" => {
            write(
                out,
                format!(
                    "Action: {}\nAccess: {}",
                    text(value, "action_id"),
                    text(value, "access")
                ),
            )?;
            author_text(&value["metadata"], out, 0)?;
            if let Some(parameters) = value["parameters"].as_array().filter(|v| !v.is_empty()) {
                write(out, "\nParameters")?;
                for p in parameters {
                    let default = if p["default_redacted"].as_bool() == Some(true) {
                        "[redacted]".into()
                    } else {
                        p.pointer("/default_value/value")
                            .map(scalar)
                            .unwrap_or_else(|| "None".into())
                    };
                    write(
                        out,
                        format!(
                            "  {} ({})\n    Default: {default}",
                            text(p, "parameter_id"),
                            text(p, "parameter_type")
                        ),
                    )?;
                    author_text(&p["metadata"], out, 4)?;
                }
            }
            if let Some(outputs) = value["outputs"].as_array().filter(|v| !v.is_empty()) {
                write(out, "\nOutputs")?;
                for output in outputs {
                    write(out, format!("  Output: {}", text(output, "output_id")))?;
                    author_text(&output["metadata"], out, 4)?;
                }
            }
            return Ok(());
        }
        "run list" => {
            let items = value["items"]
                .as_array()
                .ok_or_else(|| CliError::operation("invalid Run list result"))?;
            if items.is_empty() {
                return write(out, "No Runs.");
            }
            table(
                &["RUN", "INSTANCE", "OPERATION", "OUTCOME"],
                items
                    .iter()
                    .map(|item| {
                        vec![
                            display_id(item, "run_id", "run_prefix_length", display),
                            display_id(item, "instance_id", "instance_prefix_length", display),
                            operation(&item["operation"]),
                            item.pointer("/state/outcome")
                                .map(scalar)
                                .unwrap_or_else(|| text(&item["state"], "phase")),
                        ]
                    })
                    .collect(),
                out,
            )?;
            return page_footer(value, display, out);
        }
        "instance history list" | "instance deletion list" => {
            let items = value["items"]
                .as_array()
                .ok_or_else(|| CliError::operation("invalid history result"))?;
            if items.is_empty() {
                return write(out, "No Instance history.");
            }
            table(
                &[
                    "INSTANCE ID",
                    "CURRENT NAME",
                    "RECORDED NAME",
                    "MANAGEMENT",
                    "DELETION",
                ],
                items
                    .iter()
                    .map(|item| {
                        vec![
                            display_id(item, "instance_id", "unique_prefix_length", display),
                            item["current_name"]
                                .as_str()
                                .map(catalog::safe)
                                .unwrap_or_else(|| "-".into()),
                            text(item, "recorded_name"),
                            if item["current_name"].is_string() {
                                "Managed"
                            } else if item["retired"].as_bool() == Some(true) {
                                "Retired"
                            } else {
                                "Not live"
                            }
                            .into(),
                            item["deletion_phase"]
                                .as_str()
                                .map(label)
                                .unwrap_or_else(|| "None".into()),
                        ]
                    })
                    .collect(),
                out,
            )?;
            return page_footer(value, display, out);
        }
        "snapshot list" => {
            let items = value["items"]
                .as_array()
                .ok_or_else(|| CliError::operation("invalid Snapshot result"))?;
            if items.is_empty() {
                return write(out, "No Snapshots.");
            }
            return table(
                &["SNAPSHOT ID", "ORIGIN INSTANCE", "CAPTURED (UTC)"],
                items
                    .iter()
                    .map(|item| {
                        vec![
                            display_id(item, "snapshot_id", "unique_prefix_length", display),
                            text(item, "origin_instance_id"),
                            captured_time(&item["captured_at"]),
                        ]
                    })
                    .collect(),
                out,
            );
        }
        "instance migration-paths" => {
            write(out, "Migration paths")?;
            let candidates = value["candidates"]
                .as_array()
                .ok_or_else(|| CliError::operation("invalid Migration paths result"))?;
            for candidate in candidates {
                let id = text(candidate, "path_id");
                let width = candidate["unique_prefix_length"].as_u64().unwrap_or(64) as usize;
                let shown = if !display.no_trunc && width < 64 {
                    &id[..4 + width.max(12)]
                } else {
                    &id
                };
                write(
                    out,
                    format!(
                        "\nPath ID: {shown}\nEdges: {}",
                        text(candidate, "edge_count")
                    ),
                )?;
                if let Some(revisions) = candidate["revisions"].as_array() {
                    let steps: Vec<_> = revisions
                        .windows(2)
                        .enumerate()
                        .map(|(index, pair)| {
                            vec![
                                (index + 1).to_string(),
                                migration_summary(value, &pair[0], &pair[1]),
                            ]
                        })
                        .collect();
                    if steps.iter().any(|row| !row[1].is_empty()) {
                        table(&["STEP", "SUMMARY"], steps, out)?;
                    }
                }
            }
            if candidates.is_empty() {
                write(out, "No Migration paths available.")?;
            }
            if let Some(next) = value["next_after"]
                .as_str()
                .and_then(|c| display.continuation(c))
            {
                write(
                    out,
                    format!("\nMore results available. Continue:\n  {next}"),
                )?;
            }
            return Ok(());
        }
        "service-storage detached list" => {
            let items = value["items"]
                .as_array()
                .ok_or_else(|| CliError::operation("invalid detached allocation result"))?;
            if items.is_empty() {
                return write(out, "No detached allocations.");
            }
            return table(
                &["ALLOCATION", "FORMER INSTANCE", "STATE", "ORIGIN STORAGE"],
                items
                    .iter()
                    .map(|item| {
                        vec![
                            display_id(item, "allocation_id", "unique_prefix_length", display),
                            text(item, "former_instance_id"),
                            text(item, "state"),
                            text(item, "origin_storage_id"),
                        ]
                    })
                    .collect(),
                out,
            );
        }
        "instance delete" | "instance abandon" if value["admission"] == "not_attempted" => {
            write(out, "Mode: preview (no Run created)")?;
            return tree(value, out, 2);
        }
        "invoke" if value["preview"] == "not_admitted" => {
            write(
                out,
                format!(
                    "Action preview: {}\nInstance ID: {}\nAccess: {}",
                    text(value, "action_id"),
                    text(value, "instance_id"),
                    text(value, "access")
                ),
            )?;
            write(
                out,
                "Mode: preview (no Run created; admission not attempted)",
            )?;
            write(
                out,
                format!(
                    "Expected state version: {}",
                    text(value, "expected_state_version")
                ),
            )?;
            if let Some(missing) = value["missing_input_ids"]
                .as_array()
                .filter(|ids| !ids.is_empty())
            {
                write(
                    out,
                    format!(
                        "Missing required Inputs: {}",
                        missing.iter().map(scalar).collect::<Vec<_>>().join(", ")
                    ),
                )?;
                write(
                    out,
                    "Next: bind these Inputs, then preview again before invoking.",
                )?;
            } else if value["required_inputs_satisfied"] == true {
                write(
                    out,
                    "Required inputs: Satisfied (checked again at admission)",
                )?;
            } else {
                write(
                    out,
                    "Required inputs: not satisfied (missing IDs unavailable)",
                )?;
            }
            let revision = &value["revision"];
            write(
                out,
                format!(
                    "Revision: {}:{}",
                    text(revision, "package_id"),
                    text(revision, "content_digest").trim_start_matches("sha256:")
                ),
            )?;
            if let Some(launch) = value.get("launch") {
                write(out, "Launch")?;
                tree(launch, out, 2)?;
            }
            write(out, format!("Terminal: {}", text(value, "terminal")))?;
            if let Some(parameters) = value["parameters"].as_array().filter(|p| !p.is_empty()) {
                write(
                    out,
                    format!(
                        "Parameters: {}",
                        parameters
                            .iter()
                            .map(|p| text(p, "parameter_id"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )?;
            }
            return Ok(());
        }
        "instance migrate" | "snapshot capture" | "snapshot restore"
            if value["preview"] == "not_admitted" =>
        {
            write(out, "Mode: preview (no Run created)")?;
            return tree(value, out, 2);
        }
        "run show" => return run_summary(value, out, true),
        "instance abandon" if value.get("run").is_some() => {
            write(
                out,
                "Abandon preserves remaining service data. Stop external service processes separately.",
            )?;
            return run_summary(value, out, false);
        }
        "invoke" | "snapshot capture" | "snapshot restore" | "instance delete"
        | "instance abandon"
            if value.get("run").is_some() =>
        {
            return run_summary(value, out, false);
        }
        "instance migrate" if value.get("inspection").is_some() => {
            run_summary(&value["inspection"], out, false)?;
            write(out, "\nCurrent instance")?;
            return instance(&value["current_instance"], out);
        }
        "revision show" | "revision metadata show" => {
            write(
                out,
                format!(
                    "Revision: {}\nInstalled: {}\nCore format: {}",
                    text(value, "reference"),
                    installation_time(value),
                    text(value, "core_version")
                ),
            )?;
            write(
                out,
                format!(
                    "Identity: {}:{}",
                    text(&value["revision"], "package_id"),
                    text(&value["revision"], "content_digest").trim_start_matches("sha256:")
                ),
            )?;
            if command == "revision metadata show" {
                write(out, "\nMetadata")?;
                return tree(&value["metadata"], out, 2);
            }
            let declarations = &value["declarations"];
            let capabilities = &declarations["capabilities"];
            author_text(&capabilities["metadata"], out, 0)?;
            let mut has_capabilities = false;
            if let Some(actions) = declarations["actions"].as_array().filter(|v| !v.is_empty()) {
                has_capabilities = true;
                write(out, "\nActions")?;
                for action in actions {
                    let mut action = action.clone();
                    if let Some(fields) = action.as_object_mut() {
                        for repeated in ["launch", "terminal", "protocol_version", "output_ids"] {
                            fields.remove(repeated);
                        }
                    }
                    tree(&action, out, 2)?;
                }
            }
            for key in [
                "inputs",
                "capture",
                "restore",
                "migrations",
                "cleanup",
                "service_storages",
                "service_resources",
            ] {
                if let Some(capability) = capabilities
                    .get(key)
                    .filter(|v| !v.is_null() && !matches!(v,Value::Array(a) if a.is_empty()))
                {
                    has_capabilities = true;
                    write(out, format!("\n{}", label(key)))?;
                    tree(capability, out, 2)?;
                }
            }
            if !has_capabilities {
                write(
                    out,
                    "\nNo Actions, Inputs or managed capabilities declared.",
                )?;
            }
            if let Some(metadata) = value["metadata"].as_array() {
                let claims: Vec<_> = metadata
                    .iter()
                    .filter(|m| m["kind"] != "presentation")
                    .cloned()
                    .collect();
                if !claims.is_empty() {
                    write(out, "\nMetadata")?;
                    tree(&Value::Array(claims), out, 2)?;
                }
            }
            return Ok(());
        }
        "revision list" => {
            let items = value["items"]
                .as_array()
                .ok_or_else(|| CliError::operation("invalid Revision list result"))?;
            if items.is_empty() {
                return write(out, "No installed Revisions.");
            }
            table(
                &["REVISION", "INSTALLED (UTC)", "SUMMARY"],
                items
                    .iter()
                    .map(|item| {
                        vec![
                            if display.no_trunc {
                                format!(
                                    "{}:{}",
                                    text(&item["revision"], "package_id"),
                                    text(&item["revision"], "content_digest")
                                        .trim_start_matches("sha256:")
                                )
                            } else {
                                text(item, "reference")
                            },
                            installation_time(item),
                            summary_cell(&item["declarations"]["capabilities"]["metadata"]),
                        ]
                    })
                    .collect(),
                out,
            )?;
            return page_footer(value, display, out);
        }
        _ => {}
    }
    write(
        out,
        command
            .split_whitespace()
            .map(label)
            .collect::<Vec<_>>()
            .join(" "),
    )?;
    tree(value, out, 2)
}

#[cfg(test)]
#[path = "human_summary_tests.rs"]
mod summary_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Supporting coverage for PR-TEST-0682.
    #[test]
    fn human_author_fields_have_headings_and_safe_multiline_content() {
        let metadata = json!({
            "display_name": "Readable label",
            "summary": "A short summary.",
            "description": "First line.\r\n\r\nSecond line.\n",
            "help": "Use \u{1b}[31mcare\t."
        });
        let mut out = Vec::new();
        author_text(&metadata, &mut out, 0).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            concat!(
                "\nDisplay Name\n  Readable label\n",
                "\nSummary\n  A short summary.\n",
                "\nDescription\n  First line.\n  \n  Second line.\n",
                "\nHelp\n  Use \\u{1b}[31mcare\\t.\n"
            )
        );
    }

    // Supporting coverage for PR-TEST-0682.
    #[test]
    fn human_author_fields_omit_missing_null_and_empty_values() {
        let mut out = Vec::new();
        author_text(&json!({"summary": null, "description": ""}), &mut out, 0).unwrap();
        assert!(out.is_empty());
    }

    // Supporting coverage for PR-TEST-0682.
    #[test]
    fn human_action_parameter_fields_remain_under_the_parameter() {
        let action = json!({
            "action_id": "send", "access": "observe",
            "metadata": {"summary": "Send a message."},
            "parameters": [{
                "parameter_id": "message", "parameter_type": "string",
                "default_redacted": true, "default_value": {"value": "private-sentinel"},
                "metadata": {"help": "First line.\nSecond line."}
            }]
        });
        let mut out = Vec::new();
        render(
            "action show",
            &action,
            &reply::DisplayOptions::default(),
            &mut out,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            concat!(
                "Action: send\nAccess: observe\n",
                "\nSummary\n  Send a message.\n",
                "\nParameters\n  message (string)\n    Default: [redacted]\n",
                "\n    Help\n      First line.\n      Second line.\n"
            )
        );
    }
}
