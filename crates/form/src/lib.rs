//! The FormCraft form document: pages, fields and responses.
//!
//! This crate holds the file format, its validation and response checking. It does no IO.
//! Conditional logic lives in `formcraft-logic`; the FigCraft design binding lives in
//! `formcraft-fig`.

mod response;
mod validate;

pub use response::{Issue, Overlay, Response, check_response};
pub use validate::{ValidateError, validate};

use serde::{Deserialize, Serialize};

/// The newest form file format this build reads and writes.
pub const FORMAT_VERSION: u32 = 1;
/// Upper bound on pages. Form files are hostile input (AGENTS.md, Never crash).
pub const MAX_PAGES: usize = 256;
/// Upper bound on fields across the whole form.
pub const MAX_FIELDS: usize = 4096;
/// Upper bound on options in one choice field.
pub const MAX_OPTIONS: usize = 1024;
/// Upper bound on a text answer, in bytes, when the field sets no tighter cap.
pub const MAX_TEXT_LENGTH: usize = 64 * 1024;
/// Upper bound on a serialized form document, in bytes.
pub const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

/// A form: ordered pages of fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Form {
    pub format_version: u32,
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub pages: Vec<Page>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub help: String,
    #[serde(default)]
    pub required: bool,
    pub kind: FieldKind,
}

/// What a field asks for. Answers are checked in [`check_response`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum FieldKind {
    /// Free text. `max_length` is in bytes and may not exceed [`MAX_TEXT_LENGTH`].
    Text {
        #[serde(default)]
        multiline: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_length: Option<u32>,
    },
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
        #[serde(default)]
        integer: bool,
    },
    Choice {
        options: Vec<ChoiceOption>,
        #[serde(default)]
        multiple: bool,
    },
    /// A calendar date, answered as `"YYYY-MM-DD"`.
    Date,
    Email,
    /// A rating from 1 to `max`.
    Rating {
        max: u8,
    },
    /// An e-signature: `{"name": "...", "signed_at": "RFC 3339, optional"}`.
    Signature,
    /// Display-only text; takes no answer.
    Statement,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceOption {
    pub id: String,
    pub label: String,
}

/// Why a form document could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("form document is {0} bytes, over the 4 MiB cap")]
    TooLarge(usize),
    #[error("not valid form JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("form format version {found} is newer than this build reads (newest is {newest})")]
    UnsupportedVersion { found: u32, newest: u32 },
}

impl Form {
    /// Reads a form document, enforcing the size cap and the format version gate.
    /// Call [`validate`] afterwards before using it.
    pub fn from_json_str(text: &str) -> Result<Self, ParseError> {
        if text.len() > MAX_DOCUMENT_BYTES {
            return Err(ParseError::TooLarge(text.len()));
        }
        let form: Self = serde_json::from_str(text)?;
        if form.format_version > FORMAT_VERSION {
            return Err(ParseError::UnsupportedVersion { found: form.format_version, newest: FORMAT_VERSION });
        }
        Ok(form)
    }

    /// Writes the form document as pretty JSON.
    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Every field, in document order.
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.pages.iter().flat_map(|page| page.fields.iter())
    }

    /// The field with this id, with its page's id.
    pub fn field(&self, id: &str) -> Option<(&Page, &Field)> {
        self.pages.iter().find_map(|page| page.fields.iter().find(|field| field.id == id).map(|field| (page, field)))
    }

    /// The page with this id.
    pub fn page(&self, id: &str) -> Option<&Page> {
        self.pages.iter().find(|page| page.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::{FORMAT_VERSION, Form, ParseError};
    use serde_json::json;

    pub(crate) fn sample() -> Form {
        let value = json!({
            "format_version": FORMAT_VERSION,
            "id": "visit-survey",
            "title": "Visit survey",
            "pages": [
                {"id": "p1", "title": "About the visit", "fields": [
                    {"id": "name", "label": "Your name", "required": true, "kind": {"type": "text"}},
                    {"id": "email", "label": "Email", "kind": {"type": "email"}},
                    {"id": "visits", "label": "Visits this year", "kind": {"type": "number", "min": 0.0, "integer": true}},
                    {"id": "rating", "label": "How was it?", "required": true, "kind": {"type": "rating", "max": 5}},
                    {"id": "channel", "label": "How did you hear about us?", "kind": {"type": "choice", "multiple": false, "options": [
                        {"id": "friend", "label": "A friend"},
                        {"id": "web", "label": "The web"}
                    ]}}
                ]},
                {"id": "p2", "title": "Details", "fields": [
                    {"id": "comment", "label": "Tell us more", "kind": {"type": "text", "multiline": true, "max_length": 2000}},
                    {"id": "date", "label": "Date of the visit", "kind": {"type": "date"}},
                    {"id": "sign", "label": "Signature", "kind": {"type": "signature"}}
                ]}
            ]
        });
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn round_trips_through_json() {
        let form = sample();
        let text = form.to_json_string().unwrap();
        assert_eq!(form, Form::from_json_str(&text).unwrap());
    }

    #[test]
    fn rejects_newer_format_versions() {
        let mut form = sample();
        form.format_version = FORMAT_VERSION + 1;
        let text = form.to_json_string().unwrap();
        assert!(matches!(Form::from_json_str(&text), Err(ParseError::UnsupportedVersion { .. })));
    }

    #[test]
    fn finds_fields_and_pages() {
        let form = sample();
        assert_eq!(form.fields().count(), 8);
        let (page, field) = form.field("comment").unwrap();
        assert_eq!((page.id.as_str(), field.id.as_str()), ("p2", "comment"));
        assert!(form.page("p1").is_some());
        assert!(form.field("ghost").is_none());
    }
}
