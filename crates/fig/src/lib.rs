//! The FigCraft binding: a form designed in FigCraft, run by FormCraft.
//!
//! The binding is a sidecar document next to the design, so the design file stays
//! FigCraft's alone. It maps design node ids to form fields, pages and controls. Reading
//! FigCraft documents themselves belongs to the suite's `craft-fig` and
//! `craft-figcraft-format` crates; until this crate adopts them (docs/gaps.md), it reads a
//! plain exported node listing: a JSON array of `{"id", "name", "kind"}`.
//!
//! Designers opt in by layer name: a node called `field:email` suggests a binding to the
//! field `email`, `page:p2` to a page, `submit` to the submit control, `error:email` to
//! that field's error text. [`suggest`] turns a listing into bindings by those names.

use formcraft_form::Form;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The newest binding file format this build reads and writes.
pub const FORMAT_VERSION: u32 = 1;
/// Upper bound on bindings and on listed nodes.
pub const MAX_NODES: usize = 64 * 1024;
/// Upper bound on a serialized binding document or node listing, in bytes.
pub const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

/// The sidecar document: which design, which form, and the node bindings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingSet {
    pub format_version: u32,
    /// The FigCraft design this binds, as a relative path.
    pub fig_file: String,
    /// The form document this binds, as a relative path.
    pub form_file: String,
    #[serde(default)]
    pub bindings: Vec<NodeBinding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeBinding {
    /// The design node's id, as FigCraft reports it.
    pub node_id: String,
    /// The node's name when the binding was made; informational, for diffs and review.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub node_name: String,
    pub target: BindingTarget,
}

/// What a bound node plays in the running form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum BindingTarget {
    /// The node is the input for this field.
    Field { field: String },
    /// The node is the frame for this page.
    Page { page: String },
    /// The node submits the form.
    Submit,
    /// The node shows this field's validation message.
    ErrorText { field: String },
}

/// One node from an exported FigCraft node listing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FigNode {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: String,
}

/// Why a binding document or node listing could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("document is {0} bytes, over the 16 MiB cap")]
    TooLarge(usize),
    #[error("not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("binding format version {found} is newer than this build reads (newest is {newest})")]
    UnsupportedVersion { found: u32, newest: u32 },
    #[error("the listing has {0} nodes, over the cap of {MAX_NODES}")]
    TooManyNodes(usize),
}

impl BindingSet {
    /// Reads a binding document, enforcing the size cap and the format version gate.
    pub fn from_json_str(text: &str) -> Result<Self, ParseError> {
        if text.len() > MAX_DOCUMENT_BYTES {
            return Err(ParseError::TooLarge(text.len()));
        }
        let set: Self = serde_json::from_str(text)?;
        if set.format_version > FORMAT_VERSION {
            return Err(ParseError::UnsupportedVersion { found: set.format_version, newest: FORMAT_VERSION });
        }
        if set.bindings.len() > MAX_NODES {
            return Err(ParseError::TooManyNodes(set.bindings.len()));
        }
        Ok(set)
    }

    /// Writes the binding document as pretty JSON.
    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Reads an exported node listing.
pub fn read_node_listing(text: &str) -> Result<Vec<FigNode>, ParseError> {
    if text.len() > MAX_DOCUMENT_BYTES {
        return Err(ParseError::TooLarge(text.len()));
    }
    let nodes: Vec<FigNode> = serde_json::from_str(text)?;
    if nodes.len() > MAX_NODES {
        return Err(ParseError::TooManyNodes(nodes.len()));
    }
    Ok(nodes)
}

/// Why a binding set does not fit its form.
#[derive(Debug, thiserror::Error)]
pub enum BindingError {
    #[error("a binding has an empty node id")]
    EmptyNodeId,
    #[error("the node {0:?} is bound twice")]
    DuplicateNode(String),
    #[error("a binding names the unknown field {0:?}")]
    UnknownField(String),
    #[error("a binding names the unknown page {0:?}")]
    UnknownPage(String),
    #[error("the field {0:?} has two input bindings")]
    FieldBoundTwice(String),
    #[error("the form has two submit bindings")]
    TwoSubmits,
}

/// Checks a binding set against its form: nodes bind once, targets resolve, each field has
/// at most one input and the form at most one submit.
pub fn validate_bindings(form: &Form, set: &BindingSet) -> Result<(), BindingError> {
    let mut nodes = BTreeSet::new();
    let mut bound_fields = BTreeSet::new();
    let mut submit_seen = false;
    for binding in &set.bindings {
        if binding.node_id.is_empty() {
            return Err(BindingError::EmptyNodeId);
        }
        if !nodes.insert(binding.node_id.as_str()) {
            return Err(BindingError::DuplicateNode(binding.node_id.clone()));
        }
        match &binding.target {
            BindingTarget::Field { field } => {
                if form.field(field).is_none() {
                    return Err(BindingError::UnknownField(field.clone()));
                }
                if !bound_fields.insert(field.as_str()) {
                    return Err(BindingError::FieldBoundTwice(field.clone()));
                }
            }
            BindingTarget::ErrorText { field } => {
                if form.field(field).is_none() {
                    return Err(BindingError::UnknownField(field.clone()));
                }
            }
            BindingTarget::Page { page } => {
                if form.page(page).is_none() {
                    return Err(BindingError::UnknownPage(page.clone()));
                }
            }
            BindingTarget::Submit => {
                if submit_seen {
                    return Err(BindingError::TwoSubmits);
                }
                submit_seen = true;
            }
        }
    }
    Ok(())
}

/// Suggests bindings from layer names: `field:<id>`, `page:<id>`, `error:<id>`, `submit`.
/// Names that resolve to nothing in the form are left alone; the first node wins a name.
pub fn suggest(form: &Form, nodes: &[FigNode]) -> Vec<NodeBinding> {
    let mut taken = BTreeSet::new();
    let mut bindings = Vec::new();
    for node in nodes {
        let Some(target) = target_for(form, node.name.trim()) else {
            continue;
        };
        let slot = slot_key(&target);
        if !taken.insert(slot) {
            continue;
        }
        bindings.push(NodeBinding { node_id: node.id.clone(), node_name: node.name.clone(), target });
    }
    bindings
}

fn target_for(form: &Form, name: &str) -> Option<BindingTarget> {
    if name == "submit" {
        return Some(BindingTarget::Submit);
    }
    if let Some(field) = name.strip_prefix("field:") {
        return form.field(field).map(|_| BindingTarget::Field { field: field.to_string() });
    }
    if let Some(page) = name.strip_prefix("page:") {
        return form.page(page).map(|_| BindingTarget::Page { page: page.to_string() });
    }
    if let Some(field) = name.strip_prefix("error:") {
        return form.field(field).map(|_| BindingTarget::ErrorText { field: field.to_string() });
    }
    None
}

fn slot_key(target: &BindingTarget) -> String {
    match target {
        BindingTarget::Field { field } => format!("field:{field}"),
        BindingTarget::Page { page } => format!("page:{page}"),
        BindingTarget::ErrorText { field } => format!("error:{field}"),
        BindingTarget::Submit => "submit".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{BindingError, BindingSet, BindingTarget, FORMAT_VERSION, FigNode, NodeBinding, read_node_listing, suggest, validate_bindings};
    use formcraft_form::Form;
    use serde_json::json;

    fn form() -> Form {
        let value = json!({
            "format_version": 1,
            "id": "contact",
            "title": "Contact",
            "pages": [{"id": "p1", "fields": [
                {"id": "email", "label": "Email", "required": true, "kind": {"type": "email"}},
                {"id": "message", "label": "Message", "kind": {"type": "text", "multiline": true}}
            ]}]
        });
        serde_json::from_value(value).unwrap()
    }

    fn node(id: &str, name: &str) -> FigNode {
        FigNode { id: id.into(), name: name.into(), kind: "frame".into() }
    }

    #[test]
    fn suggests_bindings_by_layer_name() {
        let nodes =
            vec![node("1:1", "page:p1"), node("1:2", "field:email"), node("1:3", "error:email"), node("1:4", "submit"), node("1:5", "decoration")];
        let bindings = suggest(&form(), &nodes);
        assert_eq!(bindings.len(), 4);
        assert_eq!(bindings[1].target, BindingTarget::Field { field: "email".into() });
        let set = BindingSet { format_version: FORMAT_VERSION, fig_file: "contact.fig".into(), form_file: "contact.form.json".into(), bindings };
        validate_bindings(&form(), &set).unwrap();
        let text = set.to_json_string().unwrap();
        assert_eq!(set, BindingSet::from_json_str(&text).unwrap());
    }

    #[test]
    fn ignores_names_that_resolve_to_nothing() {
        let bindings = suggest(&form(), &[node("1:1", "field:ghost"), node("1:2", "field:email"), node("1:3", "field:email")]);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].node_id, "1:2");
    }

    #[test]
    fn rejects_double_bindings() {
        let twice = vec![
            NodeBinding { node_id: "a".into(), node_name: String::new(), target: BindingTarget::Field { field: "email".into() } },
            NodeBinding { node_id: "b".into(), node_name: String::new(), target: BindingTarget::Field { field: "email".into() } },
        ];
        let set = BindingSet { format_version: FORMAT_VERSION, fig_file: "f".into(), form_file: "g".into(), bindings: twice };
        assert!(matches!(validate_bindings(&form(), &set), Err(BindingError::FieldBoundTwice(_))));
    }

    #[test]
    fn reads_a_node_listing() {
        let nodes = read_node_listing(r#"[{"id": "1:1", "name": "field:email", "kind": "frame"}, {"id": "1:2", "name": "x"}]"#).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[1].kind, "");
    }
}
