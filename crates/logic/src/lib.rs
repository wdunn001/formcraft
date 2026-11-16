//! Conditional logic over a form: rules that show, hide, require, relax and jump.
//!
//! A logic set is its own document, next to the form. Rules evaluate in order against the
//! answers so far; where two rules disagree, the later one wins. Conditions are structured,
//! with no expression strings, so a builder UI can offer and validate them without a parser.
//!
//! An unanswered or wrongly shaped answer makes a comparison false rather than an error:
//! while someone is filling a form, most fields are not answered yet.

use formcraft_form::{FieldKind, Form};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The newest logic file format this build reads and writes.
pub const FORMAT_VERSION: u32 = 1;
/// Upper bound on rules. Logic files are hostile input (AGENTS.md, Never crash).
pub const MAX_RULES: usize = 4096;
/// Upper bound on condition nesting.
pub const MAX_CONDITION_DEPTH: usize = 64;
/// Upper bound on a serialized logic document, in bytes.
pub const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

/// The logic document: rules for one form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogicSet {
    pub format_version: u32,
    pub form_id: String,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub when: Condition,
    #[serde(default)]
    pub actions: Vec<RuleAction>,
}

/// A condition over the answers. Field references are by field id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Condition {
    /// The field has an answer (non-null, non-empty).
    Answered {
        field: String,
    },
    Equals {
        field: String,
        value: Value,
    },
    NotEquals {
        field: String,
        value: Value,
    },
    /// Numeric; false when the answer is missing or not a number.
    GreaterThan {
        field: String,
        value: f64,
    },
    /// Numeric; false when the answer is missing or not a number.
    LessThan {
        field: String,
        value: f64,
    },
    /// Substring of a text answer, or membership in a multi-choice answer.
    Contains {
        field: String,
        value: Value,
    },
    All {
        conditions: Vec<Condition>,
    },
    Any {
        conditions: Vec<Condition>,
    },
    Not {
        condition: Box<Condition>,
    },
}

/// What a matching rule does. Later rules override earlier ones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RuleAction {
    ShowField {
        field: String,
    },
    HideField {
        field: String,
    },
    ShowPage {
        page: String,
    },
    HidePage {
        page: String,
    },
    Require {
        field: String,
    },
    Relax {
        field: String,
    },
    /// After `after_page`, go to `page` instead of the next page in order.
    JumpToPage {
        after_page: String,
        page: String,
    },
}

/// Why a logic document could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("logic document is {0} bytes, over the 4 MiB cap")]
    TooLarge(usize),
    #[error("not valid logic JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("logic format version {found} is newer than this build reads (newest is {newest})")]
    UnsupportedVersion { found: u32, newest: u32 },
}

impl LogicSet {
    /// Reads a logic document, enforcing the size cap and the format version gate.
    /// Call [`validate_logic`] against the form afterwards.
    pub fn from_json_str(text: &str) -> Result<Self, ParseError> {
        if text.len() > MAX_DOCUMENT_BYTES {
            return Err(ParseError::TooLarge(text.len()));
        }
        let set: Self = serde_json::from_str(text)?;
        if set.format_version > FORMAT_VERSION {
            return Err(ParseError::UnsupportedVersion { found: set.format_version, newest: FORMAT_VERSION });
        }
        Ok(set)
    }

    /// Writes the logic document as pretty JSON.
    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Why a logic document does not fit its form.
#[derive(Debug, thiserror::Error)]
pub enum LogicError {
    #[error("the logic is for form {found:?}, not {expected:?}")]
    WrongForm { expected: String, found: String },
    #[error("the logic has {0} rules, over the cap of {MAX_RULES}")]
    TooManyRules(usize),
    #[error("a rule has an empty id")]
    EmptyRuleId,
    #[error("two rules share the id {0:?}")]
    DuplicateRuleId(String),
    #[error("rule {rule:?} names the unknown field {field:?}")]
    UnknownField { rule: String, field: String },
    #[error("rule {rule:?} names the unknown page {page:?}")]
    UnknownPage { rule: String, page: String },
    #[error("rule {0:?} nests conditions deeper than {MAX_CONDITION_DEPTH}")]
    ConditionTooDeep(String),
}

/// Checks a logic set against its form: ids resolve, caps hold, nesting is bounded.
pub fn validate_logic(form: &Form, set: &LogicSet) -> Result<(), LogicError> {
    if set.form_id != form.id {
        return Err(LogicError::WrongForm { expected: form.id.clone(), found: set.form_id.clone() });
    }
    if set.rules.len() > MAX_RULES {
        return Err(LogicError::TooManyRules(set.rules.len()));
    }
    let mut ids = BTreeSet::new();
    for rule in &set.rules {
        if rule.id.is_empty() {
            return Err(LogicError::EmptyRuleId);
        }
        if !ids.insert(rule.id.as_str()) {
            return Err(LogicError::DuplicateRuleId(rule.id.clone()));
        }
        check_condition(form, rule, &rule.when, 1)?;
        for action in &rule.actions {
            check_action(form, rule, action)?;
        }
    }
    Ok(())
}

fn check_condition(form: &Form, rule: &Rule, condition: &Condition, depth: usize) -> Result<(), LogicError> {
    if depth > MAX_CONDITION_DEPTH {
        return Err(LogicError::ConditionTooDeep(rule.id.clone()));
    }
    match condition {
        Condition::Answered { field }
        | Condition::Equals { field, .. }
        | Condition::NotEquals { field, .. }
        | Condition::GreaterThan { field, .. }
        | Condition::LessThan { field, .. }
        | Condition::Contains { field, .. } => check_field(form, rule, field),
        Condition::All { conditions } | Condition::Any { conditions } => {
            for nested in conditions {
                check_condition(form, rule, nested, depth + 1)?;
            }
            Ok(())
        }
        Condition::Not { condition } => check_condition(form, rule, condition, depth + 1),
    }
}

fn check_field(form: &Form, rule: &Rule, field: &str) -> Result<(), LogicError> {
    if form.field(field).is_none() {
        return Err(LogicError::UnknownField { rule: rule.id.clone(), field: field.to_string() });
    }
    Ok(())
}

fn check_action(form: &Form, rule: &Rule, action: &RuleAction) -> Result<(), LogicError> {
    match action {
        RuleAction::ShowField { field } | RuleAction::HideField { field } | RuleAction::Require { field } | RuleAction::Relax { field } => {
            check_field(form, rule, field)
        }
        RuleAction::ShowPage { page } | RuleAction::HidePage { page } => check_page(form, rule, page),
        RuleAction::JumpToPage { after_page, page } => {
            check_page(form, rule, after_page)?;
            check_page(form, rule, page)
        }
    }
}

fn check_page(form: &Form, rule: &Rule, page: &str) -> Result<(), LogicError> {
    if form.page(page).is_none() {
        return Err(LogicError::UnknownPage { rule: rule.id.clone(), page: page.to_string() });
    }
    Ok(())
}

/// What the rules decided for one set of answers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EvalState {
    pub hidden_fields: BTreeSet<String>,
    pub hidden_pages: BTreeSet<String>,
    pub required: BTreeMap<String, bool>,
    pub jumps: BTreeMap<String, String>,
}

impl EvalState {
    /// A field shows when neither it nor its page is hidden.
    pub fn field_visible(&self, form: &Form, field_id: &str) -> bool {
        match form.field(field_id) {
            None => false,
            Some((page, _)) => !self.hidden_fields.contains(field_id) && !self.hidden_pages.contains(&page.id),
        }
    }

    /// The overlay `formcraft_form::check_response` takes: hidden fields (pages folded in)
    /// and required overrides.
    pub fn overlay(&self, form: &Form) -> formcraft_form::Overlay {
        let hidden_fields = form.fields().map(|field| field.id.clone()).filter(|id| !self.field_visible(form, id)).collect();
        formcraft_form::Overlay { hidden_fields, required: self.required.clone() }
    }

    /// The page after `current`, honoring jumps and hidden pages. `None` means the end.
    pub fn next_page<'f>(&self, form: &'f Form, current: &str) -> Option<&'f formcraft_form::Page> {
        let position = form.pages.iter().position(|page| page.id == current)?;
        let start = match self.jumps.get(current) {
            Some(target) => form.pages.iter().position(|page| page.id == *target)?,
            None => position + 1,
        };
        form.pages.get(start..)?.iter().find(|page| !self.hidden_pages.contains(&page.id))
    }
}

/// Runs every rule, in order, against the answers. Call [`validate_logic`] first; this
/// function trusts the set's references.
pub fn evaluate(form: &Form, set: &LogicSet, answers: &BTreeMap<String, Value>) -> EvalState {
    let mut state = EvalState::default();
    for rule in &set.rules {
        if !holds(form, &rule.when, answers) {
            continue;
        }
        for action in &rule.actions {
            apply(action, &mut state);
        }
    }
    state
}

fn apply(action: &RuleAction, state: &mut EvalState) {
    match action {
        RuleAction::ShowField { field } => {
            state.hidden_fields.remove(field);
        }
        RuleAction::HideField { field } => {
            state.hidden_fields.insert(field.clone());
        }
        RuleAction::ShowPage { page } => {
            state.hidden_pages.remove(page);
        }
        RuleAction::HidePage { page } => {
            state.hidden_pages.insert(page.clone());
        }
        RuleAction::Require { field } => {
            state.required.insert(field.clone(), true);
        }
        RuleAction::Relax { field } => {
            state.required.insert(field.clone(), false);
        }
        RuleAction::JumpToPage { after_page, page } => {
            state.jumps.insert(after_page.clone(), page.clone());
        }
    }
}

fn holds(form: &Form, condition: &Condition, answers: &BTreeMap<String, Value>) -> bool {
    match condition {
        Condition::Answered { field } => answered(form, field, answers).is_some(),
        Condition::Equals { field, value } => answered(form, field, answers).is_some_and(|answer| answer == value),
        Condition::NotEquals { field, value } => answered(form, field, answers).is_some_and(|answer| answer != value),
        Condition::GreaterThan { field, value } => number(form, field, answers).is_some_and(|answer| answer > *value),
        Condition::LessThan { field, value } => number(form, field, answers).is_some_and(|answer| answer < *value),
        Condition::Contains { field, value } => contains(form, field, answers, value),
        Condition::All { conditions } => conditions.iter().all(|nested| holds(form, nested, answers)),
        Condition::Any { conditions } => conditions.iter().any(|nested| holds(form, nested, answers)),
        Condition::Not { condition } => !holds(form, condition, answers),
    }
}

fn answered<'a>(form: &Form, field: &str, answers: &'a BTreeMap<String, Value>) -> Option<&'a Value> {
    let value = answers.get(field).filter(|value| !value.is_null())?;
    let empty = match form.field(field).map(|(_, field)| &field.kind) {
        Some(FieldKind::Text { .. } | FieldKind::Email | FieldKind::Date) => value.as_str().is_some_and(str::is_empty),
        Some(FieldKind::Choice { multiple: true, .. }) => value.as_array().is_some_and(Vec::is_empty),
        _ => false,
    };
    if empty { None } else { Some(value) }
}

fn number(form: &Form, field: &str, answers: &BTreeMap<String, Value>) -> Option<f64> {
    answered(form, field, answers).and_then(Value::as_f64)
}

fn contains(form: &Form, field: &str, answers: &BTreeMap<String, Value>, needle: &Value) -> bool {
    match answered(form, field, answers) {
        Some(Value::String(text)) => needle.as_str().is_some_and(|sub| text.contains(sub)),
        Some(Value::Array(items)) => items.contains(needle),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{Condition, EvalState, FORMAT_VERSION, LogicError, LogicSet, Rule, RuleAction, evaluate, validate_logic};
    use formcraft_form::Form;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    fn form() -> Form {
        let value = json!({
            "format_version": 1,
            "id": "survey",
            "title": "Survey",
            "pages": [
                {"id": "p1", "fields": [
                    {"id": "rating", "label": "Rating", "required": true, "kind": {"type": "rating", "max": 5}},
                    {"id": "channels", "label": "Channels", "kind": {"type": "choice", "multiple": true, "options": [
                        {"id": "web", "label": "Web"}, {"id": "friend", "label": "Friend"}
                    ]}}
                ]},
                {"id": "p2", "fields": [
                    {"id": "comment", "label": "What went wrong?", "kind": {"type": "text", "multiline": true}}
                ]},
                {"id": "p3", "fields": [
                    {"id": "sign", "label": "Signature", "kind": {"type": "signature"}}
                ]}
            ]
        });
        serde_json::from_value(value).unwrap()
    }

    fn logic() -> LogicSet {
        LogicSet {
            format_version: FORMAT_VERSION,
            form_id: "survey".into(),
            rules: vec![
                Rule {
                    id: "happy-skips-details".into(),
                    when: Condition::GreaterThan { field: "rating".into(), value: 3.0 },
                    actions: vec![RuleAction::HidePage { page: "p2".into() }, RuleAction::JumpToPage { after_page: "p1".into(), page: "p3".into() }],
                },
                Rule {
                    id: "unhappy-must-explain".into(),
                    when: Condition::LessThan { field: "rating".into(), value: 3.0 },
                    actions: vec![RuleAction::Require { field: "comment".into() }],
                },
            ],
        }
    }

    fn answers(value: Value) -> BTreeMap<String, Value> {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn validates_and_round_trips() {
        let set = logic();
        validate_logic(&form(), &set).unwrap();
        let text = set.to_json_string().unwrap();
        assert_eq!(set, LogicSet::from_json_str(&text).unwrap());
    }

    #[test]
    fn rejects_unknown_references() {
        let mut set = logic();
        set.rules[0].actions.push(RuleAction::HideField { field: "ghost".into() });
        assert!(matches!(validate_logic(&form(), &set), Err(LogicError::UnknownField { .. })));
        let mut set = logic();
        set.form_id = "other".into();
        assert!(matches!(validate_logic(&form(), &set), Err(LogicError::WrongForm { .. })));
    }

    #[test]
    fn a_happy_rating_skips_the_details_page() {
        let form = form();
        let state = evaluate(&form, &logic(), &answers(json!({"rating": 5})));
        assert!(!state.field_visible(&form, "comment"));
        let next = state.next_page(&form, "p1").unwrap();
        assert_eq!(next.id, "p3");
        let overlay = state.overlay(&form);
        assert!(overlay.hidden_fields.contains("comment"));
    }

    #[test]
    fn an_unhappy_rating_requires_the_comment() {
        let state = evaluate(&form(), &logic(), &answers(json!({"rating": 1})));
        assert_eq!(state.required.get("comment"), Some(&true));
        assert!(state.field_visible(&form(), "comment"));
        assert_eq!(state.next_page(&form(), "p1").unwrap().id, "p2");
    }

    #[test]
    fn unanswered_conditions_are_false() {
        let state = evaluate(&form(), &logic(), &answers(json!({})));
        assert_eq!(state, EvalState::default());
    }

    #[test]
    fn later_rules_win() {
        let mut set = logic();
        set.rules.push(Rule {
            id: "override".into(),
            when: Condition::Answered { field: "rating".into() },
            actions: vec![RuleAction::ShowPage { page: "p2".into() }],
        });
        let state = evaluate(&form(), &set, &answers(json!({"rating": 5})));
        assert!(state.field_visible(&form(), "comment"));
    }

    #[test]
    fn contains_matches_multi_choice_answers() {
        let condition = Condition::Contains { field: "channels".into(), value: json!("web") };
        let set = LogicSet {
            format_version: FORMAT_VERSION,
            form_id: "survey".into(),
            rules: vec![Rule { id: "r".into(), when: condition, actions: vec![RuleAction::HideField { field: "comment".into() }] }],
        };
        let state = evaluate(&form(), &set, &answers(json!({"channels": ["friend", "web"]})));
        assert!(state.hidden_fields.contains("comment"));
        let state = evaluate(&form(), &set, &answers(json!({"channels": []})));
        assert!(state.hidden_fields.is_empty());
    }
}
