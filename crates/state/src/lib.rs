//! Headless form state: what a filling form knows between keystrokes.
//!
//! The renderer-free core in the shape of TanStack Form: per-field value and meta
//! (errors, touched, dirty, validating), a validation lifecycle (change, blur, submit)
//! where synchronous checks run before asynchronous ones, and revalidation that follows
//! the logic document's rules as answers change. Any renderer (CLI, egui, wasm) drives
//! this state; the state never draws and never does IO.
//!
//! Asynchronous checks follow the suite's submit-and-reply shape: the core queues an
//! [`AsyncCheck`] carrying a sequence number, the host runs it (lookups, server checks)
//! and answers through [`FormState::resolve_check`]; an answer with a stale sequence is
//! dropped, so a slow reply never overwrites a newer value's result. The core holds no
//! clock: debounce is a hint the host applies.

use formcraft_form::{Form, Issue, Response, check_answer, check_response, validate};
use formcraft_logic::{EvalState, LogicError, LogicSet, evaluate, validate_logic};
use serde_json::Value;
use std::collections::BTreeMap;

/// When a field's async check runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsyncTiming {
    OnChange,
    OnBlur,
}

/// A registered async check for one field. `debounce_ms` is a hint for the host; the
/// core itself holds no clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AsyncSpec {
    pub timing: AsyncTiming,
    pub debounce_ms: u32,
}

/// One check the host must run and answer with the same field and sequence.
#[derive(Clone, Debug, PartialEq)]
pub struct AsyncCheck {
    pub field: String,
    pub value: Value,
    pub seq: u64,
    pub debounce_ms: u32,
}

/// One field's meta, beside its value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FieldMeta {
    pub touched: bool,
    pub dirty: bool,
    /// Synchronous issues, as of the last change or blur.
    pub errors: Vec<Issue>,
    /// The last async answer's error, until a newer check resolves.
    pub async_error: Option<String>,
    /// An async check is in flight.
    pub validating: bool,
    seq: u64,
}

/// Why the state refused a call.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error(transparent)]
    Form(#[from] formcraft_form::ValidateError),
    #[error(transparent)]
    Logic(#[from] LogicError),
    #[error("the form has no field {0:?}")]
    UnknownField(String),
}

/// Why a submit did not produce a response.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("the form does not pass")]
pub struct SubmitBlocked {
    pub issues: Vec<Issue>,
    /// Fields whose last async check failed, with the error.
    pub async_errors: Vec<(String, String)>,
    /// Fields whose async check is still in flight.
    pub validating: Vec<String>,
}

/// The headless state of one form being filled.
pub struct FormState {
    form: Form,
    logic: Option<LogicSet>,
    eval: EvalState,
    values: BTreeMap<String, Value>,
    meta: BTreeMap<String, FieldMeta>,
    specs: BTreeMap<String, AsyncSpec>,
    pending: Vec<AsyncCheck>,
    next_seq: u64,
    submitted: bool,
}

impl FormState {
    /// Validates the form (and the logic against it) and starts empty.
    pub fn new(form: Form, logic: Option<LogicSet>) -> Result<Self, StateError> {
        validate(&form)?;
        if let Some(set) = &logic {
            validate_logic(&form, set)?;
        }
        let eval = logic.as_ref().map(|set| evaluate(&form, set, &BTreeMap::new())).unwrap_or_default();
        Ok(Self {
            form,
            logic,
            eval,
            values: BTreeMap::new(),
            meta: BTreeMap::new(),
            specs: BTreeMap::new(),
            pending: Vec::new(),
            next_seq: 0,
            submitted: false,
        })
    }

    pub fn form(&self) -> &Form {
        &self.form
    }

    /// What the logic decided for the answers so far (visibility, required, jumps).
    pub fn eval(&self) -> &EvalState {
        &self.eval
    }

    pub fn value(&self, field: &str) -> Option<&Value> {
        self.values.get(field)
    }

    /// The field's meta; a field never touched reports the default.
    pub fn meta(&self, field: &str) -> FieldMeta {
        self.meta.get(field).cloned().unwrap_or_default()
    }

    pub fn visible(&self, field: &str) -> bool {
        self.eval.field_visible(&self.form, field)
    }

    /// Required as the filler experiences it: the logic override, else the schema, and
    /// never for a hidden field.
    pub fn required(&self, field: &str) -> bool {
        if !self.visible(field) {
            return false;
        }
        let schema = self.form.field(field).map(|(_, f)| f.required).unwrap_or(false);
        self.eval.required.get(field).copied().unwrap_or(schema)
    }

    /// Registers an async check for a field.
    pub fn set_async_spec(&mut self, field: &str, spec: AsyncSpec) -> Result<(), StateError> {
        self.known(field)?;
        self.specs.insert(field.to_string(), spec);
        Ok(())
    }

    /// The change half of the lifecycle: stores the value, marks the field dirty,
    /// re-evaluates the logic, revalidates this field and every other field already in
    /// play (that is how a rule on this answer updates another field's required error),
    /// and queues the field's on-change async check once its sync checks pass.
    pub fn set_value(&mut self, field: &str, value: Value) -> Result<(), StateError> {
        self.known(field)?;
        if value.is_null() {
            self.values.remove(field);
        } else {
            self.values.insert(field.to_string(), value);
        }
        self.meta.entry(field.to_string()).or_default().dirty = true;
        self.reevaluate();
        self.revalidate_all();
        self.queue_async(field, AsyncTiming::OnChange);
        Ok(())
    }

    /// The blur half of the lifecycle: marks the field touched (required errors show from
    /// here on), revalidates, and queues the field's on-blur async check.
    pub fn blur(&mut self, field: &str) -> Result<(), StateError> {
        self.known(field)?;
        self.meta.entry(field.to_string()).or_default().touched = true;
        self.revalidate_all();
        self.queue_async(field, AsyncTiming::OnBlur);
        Ok(())
    }

    /// Drains the checks the host must run.
    pub fn pending_checks(&mut self) -> Vec<AsyncCheck> {
        std::mem::take(&mut self.pending)
    }

    /// The host's answer to one check. A stale sequence (a newer value was entered since)
    /// is dropped.
    pub fn resolve_check(&mut self, field: &str, seq: u64, error: Option<String>) {
        if let Some(meta) = self.meta.get_mut(field)
            && meta.seq == seq
        {
            meta.validating = false;
            meta.async_error = error;
        }
    }

    /// The submit half of the lifecycle: touches everything, strips answers the logic has
    /// hidden, and checks the whole response. Blocked while any async check is in flight
    /// or failed.
    pub fn submit(&mut self) -> Result<Response, SubmitBlocked> {
        self.submitted = true;
        let ids: Vec<String> = self.form.fields().map(|field| field.id.clone()).collect();
        for id in &ids {
            self.meta.entry(id.clone()).or_default().touched = true;
        }
        self.reevaluate();
        self.values.retain(|field, _| self.eval.field_visible(&self.form, field));
        self.reevaluate();
        self.revalidate_all();
        let response = Response { form_id: self.form.id.clone(), answers: self.values.clone() };
        let issues = check_response(&self.form, &response, &self.eval.overlay(&self.form));
        let async_errors: Vec<(String, String)> =
            self.meta.iter().filter_map(|(field, meta)| meta.async_error.clone().map(|error| (field.clone(), error))).collect();
        let validating: Vec<String> = self.meta.iter().filter(|(_, meta)| meta.validating).map(|(field, _)| field.clone()).collect();
        if issues.is_empty() && async_errors.is_empty() && validating.is_empty() {
            Ok(response)
        } else {
            Err(SubmitBlocked { issues, async_errors, validating })
        }
    }

    fn known(&self, field: &str) -> Result<(), StateError> {
        if self.form.field(field).is_none() {
            return Err(StateError::UnknownField(field.to_string()));
        }
        Ok(())
    }

    fn reevaluate(&mut self) {
        self.eval = self.logic.as_ref().map(|set| evaluate(&self.form, set, &self.values)).unwrap_or_default();
    }

    /// Recomputes sync errors for every field in play. A hidden field's slate is wiped:
    /// errors, async state and pending checks all go.
    fn revalidate_all(&mut self) {
        let fields: Vec<String> = self.meta.keys().cloned().collect();
        for field in fields {
            self.revalidate(&field);
        }
    }

    fn revalidate(&mut self, field: &str) {
        let Some((_, def)) = self.form.field(field) else { return };
        let def = def.clone();
        let visible = self.visible(field);
        let show_required = self.required(field) && (self.submitted || self.meta.get(field).is_some_and(|meta| meta.touched));
        let errors = if visible { check_answer(&def, self.values.get(field), show_required) } else { Vec::new() };
        let Some(meta) = self.meta.get_mut(field) else { return };
        meta.errors = errors;
        if !visible {
            meta.validating = false;
            meta.async_error = None;
            meta.seq = meta.seq.wrapping_add(1);
            self.pending.retain(|check| check.field != field);
        }
    }

    /// Queues the field's async check when one is registered for `timing`, the field is
    /// visible, its sync checks pass, and it has an answer. A newer check replaces an
    /// older pending one for the same field.
    fn queue_async(&mut self, field: &str, timing: AsyncTiming) {
        let Some(spec) = self.specs.get(field).copied() else { return };
        if spec.timing != timing || !self.visible(field) {
            return;
        }
        let Some(value) = self.values.get(field).cloned() else { return };
        if self.meta.get(field).is_some_and(|meta| !meta.errors.is_empty()) {
            return;
        }
        self.next_seq = self.next_seq.wrapping_add(1);
        let seq = self.next_seq;
        let Some(meta) = self.meta.get_mut(field) else { return };
        meta.validating = true;
        meta.async_error = None;
        meta.seq = seq;
        self.pending.retain(|check| check.field != field);
        self.pending.push(AsyncCheck { field: field.to_string(), value, seq, debounce_ms: spec.debounce_ms });
    }
}

#[cfg(test)]
mod tests {
    use super::{AsyncSpec, AsyncTiming, FormState};
    use formcraft_form::{Form, Issue};
    use formcraft_logic::LogicSet;
    use serde_json::json;

    fn form() -> Form {
        serde_json::from_value(json!({
            "format_version": 1,
            "id": "survey",
            "title": "Survey",
            "pages": [
                {"id": "p1", "fields": [
                    {"id": "name", "label": "Name", "required": true, "kind": {"type": "text"}},
                    {"id": "email", "label": "Email", "kind": {"type": "email"}},
                    {"id": "rating", "label": "Rating", "required": true, "kind": {"type": "rating", "max": 5}}
                ]},
                {"id": "p2", "fields": [
                    {"id": "comment", "label": "What went wrong?", "kind": {"type": "text", "multiline": true}}
                ]}
            ]
        }))
        .unwrap()
    }

    fn logic() -> LogicSet {
        serde_json::from_value(json!({
            "format_version": 1,
            "form_id": "survey",
            "rules": [
                {"id": "happy-hides-details",
                 "when": {"type": "greater-than", "field": "rating", "value": 3.0},
                 "actions": [{"type": "hide-page", "page": "p2"}]},
                {"id": "unhappy-must-explain",
                 "when": {"type": "less-than", "field": "rating", "value": 3.0},
                 "actions": [{"type": "require", "field": "comment"}]}
            ]
        }))
        .unwrap()
    }

    fn state() -> FormState {
        FormState::new(form(), Some(logic())).unwrap()
    }

    #[test]
    fn required_errors_wait_for_touch_or_submit() {
        let mut s = state();
        s.set_value("email", json!("ana@example.com")).unwrap();
        assert!(s.meta("name").errors.is_empty(), "untouched required field stays quiet");
        s.blur("name").unwrap();
        assert_eq!(s.meta("name").errors, vec![Issue::MissingRequired("name".into())]);
        s.set_value("name", json!("Ana")).unwrap();
        assert!(s.meta("name").errors.is_empty());
        assert!(s.meta("name").dirty);
        assert!(s.meta("name").touched);
    }

    #[test]
    fn shape_errors_show_on_change() {
        let mut s = state();
        s.set_value("email", json!("not-an-email")).unwrap();
        assert_eq!(s.meta("email").errors, vec![Issue::NotAnEmail("email".into())]);
    }

    #[test]
    fn logic_requires_and_hides_across_fields() {
        let mut s = state();
        s.set_value("rating", json!(1)).unwrap();
        assert!(s.required("comment"), "an unhappy rating requires the comment");
        let blocked = s.submit().unwrap_err();
        assert!(blocked.issues.contains(&Issue::MissingRequired("comment".into())));
        assert!(s.meta("comment").errors.contains(&Issue::MissingRequired("comment".into())), "the comment field carries the error after submit");

        s.set_value("comment", json!("It rained.")).unwrap();
        s.set_value("rating", json!(5)).unwrap();
        assert!(!s.visible("comment"), "a happy rating hides the details page");
        s.set_value("name", json!("Ana")).unwrap();
        let response = s.submit().unwrap();
        assert!(!response.answers.contains_key("comment"), "hidden answers are stripped");
    }

    #[test]
    fn async_checks_queue_resolve_and_drop_stale_answers() {
        let mut s = state();
        s.set_async_spec("name", AsyncSpec { timing: AsyncTiming::OnChange, debounce_ms: 200 }).unwrap();
        s.set_value("name", json!("Ana")).unwrap();
        let checks = s.pending_checks();
        assert_eq!(checks.len(), 1);
        assert!(s.meta("name").validating);

        s.set_value("name", json!("Anabel")).unwrap();
        let newer = s.pending_checks();
        assert_eq!(newer.len(), 1);
        s.resolve_check("name", checks[0].seq, Some("taken".into()));
        assert!(s.meta("name").validating, "a stale answer is dropped");
        assert_eq!(s.meta("name").async_error, None);

        s.resolve_check("name", newer[0].seq, Some("taken".into()));
        assert!(!s.meta("name").validating);
        s.set_value("rating", json!(5)).unwrap();
        let blocked = s.submit().unwrap_err();
        assert_eq!(blocked.async_errors, vec![("name".to_string(), "taken".to_string())]);

        s.set_value("name", json!("Ana B")).unwrap();
        let last = s.pending_checks();
        s.resolve_check("name", last[0].seq, None);
        assert!(s.submit().is_ok());
    }

    #[test]
    fn async_waits_for_sync_to_pass_and_submit_blocks_while_in_flight() {
        let mut s = state();
        s.set_async_spec("email", AsyncSpec { timing: AsyncTiming::OnBlur, debounce_ms: 0 }).unwrap();
        s.set_value("email", json!("bad")).unwrap();
        s.blur("email").unwrap();
        assert!(s.pending_checks().is_empty(), "sync failure keeps the async check back");

        s.set_value("email", json!("ana@example.com")).unwrap();
        s.blur("email").unwrap();
        assert_eq!(s.pending_checks().len(), 1);
        s.set_value("name", json!("Ana")).unwrap();
        s.set_value("rating", json!(5)).unwrap();
        let blocked = s.submit().unwrap_err();
        assert_eq!(blocked.validating, vec!["email".to_string()]);
    }

    #[test]
    fn unknown_fields_are_refused() {
        let mut s = state();
        assert!(s.set_value("ghost", json!(1)).is_err());
        assert!(s.blur("ghost").is_err());
    }
}
