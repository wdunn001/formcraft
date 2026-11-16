//! Responses and their checking against a form.

use crate::{Field, FieldKind, Form, MAX_TEXT_LENGTH};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// One filled form. Answers are keyed by field id; the shapes per kind are documented on
/// [`FieldKind`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub form_id: String,
    #[serde(default)]
    pub answers: BTreeMap<String, Value>,
}

/// What conditional logic decided for this response: hidden fields and required overrides.
/// `formcraft-logic` produces one; without logic, use the default.
#[derive(Debug, Clone, Default)]
pub struct Overlay {
    pub hidden_fields: BTreeSet<String>,
    pub required: BTreeMap<String, bool>,
}

/// One problem with a response. Checking returns every issue, so a form UI can mark all of
/// them at once.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Issue {
    #[error("the response answers form {found:?}, not {expected:?}")]
    WrongForm { expected: String, found: String },
    #[error("answer for the unknown field {0:?}")]
    UnknownField(String),
    #[error("answer for the hidden field {0:?}")]
    HiddenFieldAnswered(String),
    #[error("the required field {0:?} has no answer")]
    MissingRequired(String),
    #[error("field {0:?} wants text")]
    NotText(String),
    #[error("field {0:?} takes at most {1} bytes")]
    TextTooLong(String, usize),
    #[error("field {0:?} takes a single line")]
    SingleLineOnly(String),
    #[error("field {0:?} wants a number")]
    NotANumber(String),
    #[error("field {0:?} wants a whole number")]
    NotAnInteger(String),
    #[error("field {0:?} is outside its range")]
    OutOfRange(String),
    #[error("field {0:?} wants one option id")]
    NotAChoice(String),
    #[error("field {0:?} wants a list of option ids")]
    NotAChoiceList(String),
    #[error("field {0:?} has no option {1:?}")]
    UnknownOption(String, String),
    #[error("field {0:?} repeats the option {1:?}")]
    RepeatedOption(String, String),
    #[error("field {0:?} wants a date as YYYY-MM-DD")]
    NotADate(String),
    #[error("field {0:?} wants an email address")]
    NotAnEmail(String),
    #[error("field {0:?} wants a rating from 1 to {1}")]
    RatingOutOfRange(String, u8),
    #[error("field {0:?} wants a signature object with a non-empty name")]
    NotASignature(String),
    #[error("field {0:?} has a signed_at that is not RFC 3339")]
    BadSignatureTime(String),
    #[error("field {0:?} is a statement; it takes no answer")]
    AnsweredStatement(String),
}

/// Checks a response against a form, with the logic overlay if any. Returns every issue
/// found; an empty list means the response is good.
pub fn check_response(form: &Form, response: &Response, overlay: &Overlay) -> Vec<Issue> {
    let mut issues = Vec::new();
    if response.form_id != form.id {
        issues.push(Issue::WrongForm { expected: form.id.clone(), found: response.form_id.clone() });
    }
    let known: BTreeSet<&str> = form.fields().map(|field| field.id.as_str()).collect();
    for key in response.answers.keys() {
        if !known.contains(key.as_str()) {
            issues.push(Issue::UnknownField(key.clone()));
        }
    }
    for field in form.fields() {
        check_field_answer(field, response.answers.get(&field.id), overlay, &mut issues);
    }
    issues
}

fn check_field_answer(field: &Field, answer: Option<&Value>, overlay: &Overlay, issues: &mut Vec<Issue>) {
    let answer = answer.filter(|value| !value.is_null()).filter(|value| !is_empty_answer(&field.kind, value));
    if overlay.hidden_fields.contains(&field.id) {
        if answer.is_some() {
            issues.push(Issue::HiddenFieldAnswered(field.id.clone()));
        }
        return;
    }
    let required = overlay.required.get(&field.id).copied().unwrap_or(field.required);
    match answer {
        None => {
            if required {
                issues.push(Issue::MissingRequired(field.id.clone()));
            }
        }
        Some(value) => check_kind(field, value, issues),
    }
}

fn is_empty_answer(kind: &FieldKind, value: &Value) -> bool {
    match kind {
        FieldKind::Text { .. } | FieldKind::Email | FieldKind::Date => value.as_str().is_some_and(str::is_empty),
        FieldKind::Choice { multiple: true, .. } => value.as_array().is_some_and(Vec::is_empty),
        _ => false,
    }
}

fn check_kind(field: &Field, value: &Value, issues: &mut Vec<Issue>) {
    let id = &field.id;
    match &field.kind {
        FieldKind::Text { multiline, max_length } => check_text(id, value, *multiline, *max_length, issues),
        FieldKind::Number { min, max, integer } => check_number(id, value, *min, *max, *integer, issues),
        FieldKind::Choice { options, multiple } => check_choice(id, value, options, *multiple, issues),
        FieldKind::Date => {
            let good = value.as_str().is_some_and(|text| chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").is_ok());
            if !good {
                issues.push(Issue::NotADate(id.clone()));
            }
        }
        FieldKind::Email => {
            if !value.as_str().is_some_and(looks_like_email) {
                issues.push(Issue::NotAnEmail(id.clone()));
            }
        }
        FieldKind::Rating { max } => {
            let good = value.as_u64().is_some_and(|rating| (1..=u64::from(*max)).contains(&rating));
            if !good {
                issues.push(Issue::RatingOutOfRange(id.clone(), *max));
            }
        }
        FieldKind::Signature => check_signature(id, value, issues),
        FieldKind::Statement => issues.push(Issue::AnsweredStatement(id.clone())),
    }
}

fn check_text(id: &str, value: &Value, multiline: bool, max_length: Option<u32>, issues: &mut Vec<Issue>) {
    let Some(text) = value.as_str() else {
        issues.push(Issue::NotText(id.to_string()));
        return;
    };
    let cap = max_length.map(|cap| cap as usize).unwrap_or(MAX_TEXT_LENGTH);
    if text.len() > cap {
        issues.push(Issue::TextTooLong(id.to_string(), cap));
    }
    if !multiline && text.contains('\n') {
        issues.push(Issue::SingleLineOnly(id.to_string()));
    }
}

fn check_number(id: &str, value: &Value, min: Option<f64>, max: Option<f64>, integer: bool, issues: &mut Vec<Issue>) {
    let Some(number) = value.as_f64().filter(|_| value.is_number()) else {
        issues.push(Issue::NotANumber(id.to_string()));
        return;
    };
    if integer && number.fract() != 0.0 {
        issues.push(Issue::NotAnInteger(id.to_string()));
    }
    if min.is_some_and(|min| number < min) || max.is_some_and(|max| number > max) {
        issues.push(Issue::OutOfRange(id.to_string()));
    }
}

fn check_choice(id: &str, value: &Value, options: &[crate::ChoiceOption], multiple: bool, issues: &mut Vec<Issue>) {
    let known: BTreeSet<&str> = options.iter().map(|option| option.id.as_str()).collect();
    if multiple {
        let Some(items) = value.as_array() else {
            issues.push(Issue::NotAChoiceList(id.to_string()));
            return;
        };
        let mut seen = BTreeSet::new();
        for item in items {
            let Some(option) = item.as_str() else {
                issues.push(Issue::NotAChoiceList(id.to_string()));
                continue;
            };
            if !known.contains(option) {
                issues.push(Issue::UnknownOption(id.to_string(), option.to_string()));
            } else if !seen.insert(option) {
                issues.push(Issue::RepeatedOption(id.to_string(), option.to_string()));
            }
        }
        return;
    }
    match value.as_str() {
        None => issues.push(Issue::NotAChoice(id.to_string())),
        Some(option) if !known.contains(option) => issues.push(Issue::UnknownOption(id.to_string(), option.to_string())),
        Some(_) => {}
    }
}

fn check_signature(id: &str, value: &Value, issues: &mut Vec<Issue>) {
    let name = value.get("name").and_then(Value::as_str);
    if !value.is_object() || name.is_none_or(str::is_empty) {
        issues.push(Issue::NotASignature(id.to_string()));
        return;
    }
    if let Some(stamp) = value.get("signed_at") {
        let good = stamp.as_str().is_some_and(|text| chrono::DateTime::parse_from_rfc3339(text).is_ok());
        if !good {
            issues.push(Issue::BadSignatureTime(id.to_string()));
        }
    }
}

/// A deliberately small shape check: non-empty local part, one `@`, a dot inside the domain,
/// no whitespace, at most 254 bytes. Deliverability is the mail system's problem.
fn looks_like_email(text: &str) -> bool {
    if text.len() > 254 || text.chars().any(char::is_whitespace) {
        return false;
    }
    let Some((local, domain)) = text.split_once('@') else {
        return false;
    };
    !local.is_empty() && domain.len() > 2 && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.') && !domain.contains('@')
}

#[cfg(test)]
mod tests {
    use super::{Issue, Overlay, Response, check_response};
    use crate::tests::sample;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    fn respond(answers: Value) -> Response {
        let answers: BTreeMap<String, Value> = serde_json::from_value(answers).unwrap();
        Response { form_id: "visit-survey".into(), answers }
    }

    #[test]
    fn accepts_a_good_response() {
        let response = respond(json!({
            "name": "Ana", "email": "ana@example.com", "visits": 3, "rating": 5,
            "channel": "friend", "comment": "Great visit.\nCame twice.", "date": "2026-11-15",
            "sign": {"name": "Ana", "signed_at": "2026-11-15T10:00:00Z"}
        }));
        assert_eq!(check_response(&sample(), &response, &Overlay::default()), Vec::new());
    }

    #[test]
    fn reports_missing_required_and_bad_shapes() {
        let response = respond(json!({
            "email": "not-an-email", "visits": 2.5, "rating": 9, "channel": "radio", "date": "15.11.2026"
        }));
        let issues = check_response(&sample(), &response, &Overlay::default());
        for wanted in [
            Issue::MissingRequired("name".into()),
            Issue::NotAnEmail("email".into()),
            Issue::NotAnInteger("visits".into()),
            Issue::RatingOutOfRange("rating".into(), 5),
            Issue::UnknownOption("channel".into(), "radio".into()),
            Issue::NotADate("date".into()),
        ] {
            assert!(issues.contains(&wanted), "missing {wanted:?} in {issues:?}");
        }
    }

    #[test]
    fn the_overlay_hides_and_requires() {
        let mut overlay = Overlay::default();
        overlay.hidden_fields.insert("rating".into());
        overlay.required.insert("comment".into(), true);
        let response = respond(json!({"name": "Ana", "rating": 5}));
        let issues = check_response(&sample(), &response, &overlay);
        assert!(issues.contains(&Issue::HiddenFieldAnswered("rating".into())));
        assert!(issues.contains(&Issue::MissingRequired("comment".into())));
        assert!(!issues.contains(&Issue::MissingRequired("rating".into())));
    }

    #[test]
    fn empty_answers_count_as_missing() {
        let response = respond(json!({"name": ""}));
        let issues = check_response(&sample(), &response, &Overlay::default());
        assert!(issues.contains(&Issue::MissingRequired("name".into())));
        assert!(!issues.iter().any(|issue| matches!(issue, Issue::NotText(_))));
    }

    #[test]
    fn unknown_fields_and_wrong_form_are_reported() {
        let mut response = respond(json!({"ghost": 1}));
        response.form_id = "other".into();
        let issues = check_response(&sample(), &response, &Overlay::default());
        assert!(issues.contains(&Issue::UnknownField("ghost".into())));
        assert!(issues.iter().any(|issue| matches!(issue, Issue::WrongForm { .. })));
    }
}
