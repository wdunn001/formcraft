//! The FormCraft command line: validate, preview, fill and export forms.
//!
//! This binary owns the file IO the library crates refuse.

use clap::{Parser, Subcommand};
use formcraft_fig::{BindingSet, read_node_listing, suggest, validate_bindings};
use formcraft_form::{Form, Response, check_response, validate};
use formcraft_logic::{EvalState, LogicSet, evaluate, validate_logic};
use serde_json::Value;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "formcraft-cli", version, about = "Forms and surveys for the Craft suite")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Write a starter form document.
    New { file: PathBuf },
    /// Check a form, and with it the logic and the design bindings.
    Validate {
        form: PathBuf,
        #[arg(long)]
        logic: Option<PathBuf>,
        #[arg(long)]
        bindings: Option<PathBuf>,
    },
    /// Show the form as a filler would see it, given the answers so far.
    Preview {
        form: PathBuf,
        #[arg(long)]
        logic: Option<PathBuf>,
        /// JSON file with the answers so far.
        #[arg(long)]
        answers: Option<PathBuf>,
    },
    /// Check a full set of answers and write the response document.
    Fill {
        form: PathBuf,
        #[arg(long)]
        logic: Option<PathBuf>,
        /// JSON file with the answers.
        #[arg(long)]
        answers: PathBuf,
        /// Where to write the response; the default is standard output.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Suggest design bindings from a FigCraft node listing, by layer name.
    FigSuggest {
        form: PathBuf,
        /// JSON file with the exported node listing.
        #[arg(long)]
        nodes: PathBuf,
        /// The design file the bindings are for.
        #[arg(long, default_value = "design.fig")]
        fig_file: String,
        /// Where to write the binding set; the default is standard output.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Write responses as one CSV table.
    Export {
        form: PathBuf,
        /// Response documents, one file each.
        responses: Vec<PathBuf>,
        /// Where to write the CSV; the default is standard output.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    match dispatch(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            let mut cause = error.source();
            while let Some(inner) = cause {
                eprintln!("  caused by: {inner}");
                cause = inner.source();
            }
            ExitCode::FAILURE
        }
    }
}

fn dispatch(cli: Cli) -> Result<(), Box<dyn Error>> {
    match cli.command {
        Cmd::New { file } => new_form(&file),
        Cmd::Validate { form, logic, bindings } => validate_all(&form, logic.as_deref(), bindings.as_deref()),
        Cmd::Preview { form, logic, answers } => preview(&form, logic.as_deref(), answers.as_deref()),
        Cmd::Fill { form, logic, answers, out } => fill(&form, logic.as_deref(), &answers, out.as_deref()),
        Cmd::FigSuggest { form, nodes, fig_file, out } => fig_suggest(&form, &nodes, &fig_file, out.as_deref()),
        Cmd::Export { form, responses, out } => export(&form, &responses, out.as_deref()),
    }
}

fn load_form(path: &Path) -> Result<Form, Box<dyn Error>> {
    let form = Form::from_json_str(&std::fs::read_to_string(path)?)?;
    validate(&form)?;
    Ok(form)
}

fn load_logic(path: &Path, form: &Form) -> Result<LogicSet, Box<dyn Error>> {
    let set = LogicSet::from_json_str(&std::fs::read_to_string(path)?)?;
    validate_logic(form, &set)?;
    Ok(set)
}

fn load_answers(path: &Path) -> Result<BTreeMap<String, Value>, Box<dyn Error>> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn eval_state(form: &Form, logic: Option<&Path>, answers: &BTreeMap<String, Value>) -> Result<EvalState, Box<dyn Error>> {
    match logic {
        None => Ok(EvalState::default()),
        Some(path) => Ok(evaluate(form, &load_logic(path, form)?, answers)),
    }
}

fn emit(out: Option<&Path>, text: &str) -> Result<(), Box<dyn Error>> {
    match out {
        None => {
            println!("{text}");
            Ok(())
        }
        Some(path) => {
            std::fs::write(path, text.to_string() + "\n")?;
            println!("wrote {}", path.display());
            Ok(())
        }
    }
}

fn new_form(file: &Path) -> Result<(), Box<dyn Error>> {
    if file.exists() {
        return Err(format!("refusing to overwrite {}", file.display()).into());
    }
    let starter = serde_json::json!({
        "format_version": formcraft_form::FORMAT_VERSION,
        "id": "starter",
        "title": "Visit survey",
        "pages": [
            {"id": "p1", "title": "Your visit", "fields": [
                {"id": "rating", "label": "How was it?", "required": true, "kind": {"type": "rating", "max": 5}},
                {"id": "email", "label": "Email, if we may follow up", "kind": {"type": "email"}}
            ]},
            {"id": "p2", "title": "Details", "fields": [
                {"id": "comment", "label": "Tell us more", "kind": {"type": "text", "multiline": true, "max_length": 2000}}
            ]}
        ]
    });
    let form: Form = serde_json::from_value(starter)?;
    validate(&form)?;
    std::fs::write(file, form.to_json_string()? + "\n")?;
    println!("wrote {}", file.display());
    Ok(())
}

fn validate_all(form: &Path, logic: Option<&Path>, bindings: Option<&Path>) -> Result<(), Box<dyn Error>> {
    let form = load_form(form)?;
    if let Some(path) = logic {
        load_logic(path, &form)?;
    }
    if let Some(path) = bindings {
        let set = BindingSet::from_json_str(&std::fs::read_to_string(path)?)?;
        validate_bindings(&form, &set)?;
    }
    println!("ok");
    Ok(())
}

fn preview(form: &Path, logic: Option<&Path>, answers: Option<&Path>) -> Result<(), Box<dyn Error>> {
    let form = load_form(form)?;
    let answers = match answers {
        None => BTreeMap::new(),
        Some(path) => load_answers(path)?,
    };
    let state = eval_state(&form, logic, &answers)?;
    println!("{}  ({})", form.title, form.id);
    for page in &form.pages {
        if state.hidden_pages.contains(&page.id) {
            println!("page {}  (hidden)", page.id);
            continue;
        }
        println!("page {}  {}", page.id, page.title);
        for field in &page.fields {
            if !state.field_visible(&form, &field.id) {
                continue;
            }
            let required = state.required.get(&field.id).copied().unwrap_or(field.required);
            let marker = if required { "*" } else { " " };
            let answered = if answers.contains_key(&field.id) { "  [answered]" } else { "" };
            println!("  {marker} {}  {}{answered}", field.id, field.label);
        }
        match state.next_page(&form, &page.id) {
            Some(next) => println!("  then: {}", next.id),
            None => println!("  then: submit"),
        }
    }
    Ok(())
}

fn fill(form: &Path, logic: Option<&Path>, answers: &Path, out: Option<&Path>) -> Result<(), Box<dyn Error>> {
    let form = load_form(form)?;
    let answers = load_answers(answers)?;
    let state = eval_state(&form, logic, &answers)?;
    let response = Response { form_id: form.id.clone(), answers };
    let issues = check_response(&form, &response, &state.overlay(&form));
    if !issues.is_empty() {
        for issue in &issues {
            eprintln!("issue: {issue}");
        }
        return Err("the response does not pass; see the issues above".into());
    }
    emit(out, &serde_json::to_string_pretty(&response)?)
}

fn fig_suggest(form: &Path, nodes: &Path, fig_file: &str, out: Option<&Path>) -> Result<(), Box<dyn Error>> {
    let form_path = form;
    let form = load_form(form_path)?;
    let nodes = read_node_listing(&std::fs::read_to_string(nodes)?)?;
    let bindings = suggest(&form, &nodes);
    let set = BindingSet {
        format_version: formcraft_fig::FORMAT_VERSION,
        fig_file: fig_file.to_string(),
        form_file: form_path.display().to_string().replace('\\', "/"),
        bindings,
    };
    validate_bindings(&form, &set)?;
    emit(out, &set.to_json_string()?)
}

fn export(form: &Path, responses: &[PathBuf], out: Option<&Path>) -> Result<(), Box<dyn Error>> {
    let form = load_form(form)?;
    let fields: Vec<&str> = form.fields().map(|field| field.id.as_str()).collect();
    let mut rows = Vec::new();
    let header: Vec<String> = std::iter::once("response".to_string()).chain(fields.iter().map(|id| (*id).to_string())).collect();
    rows.push(csv_row(&header));
    for path in responses {
        let response: Response = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        if response.form_id != form.id {
            return Err(format!("{} answers form {:?}, not {:?}", path.display(), response.form_id, form.id).into());
        }
        let mut row = vec![path.display().to_string()];
        for id in &fields {
            row.push(cell(response.answers.get(*id)));
        }
        rows.push(csv_row(&row));
    }
    emit(out, rows.join("\n").as_str())
}

fn cell(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => {
            items.iter().map(|item| item.as_str().map(str::to_string).unwrap_or_else(|| item.to_string())).collect::<Vec<_>>().join("; ")
        }
        Some(other) => other.to_string(),
    }
}

fn csv_row(cells: &[String]) -> String {
    cells.iter().map(|cell| csv_escape(cell)).collect::<Vec<_>>().join(",")
}

fn csv_escape(cell: &str) -> String {
    if cell.contains(',') || cell.contains('"') || cell.contains('\n') || cell.contains('\r') {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}
