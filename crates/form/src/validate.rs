//! Structural validation of a form document.

use crate::{Field, FieldKind, Form, MAX_FIELDS, MAX_OPTIONS, MAX_PAGES, MAX_TEXT_LENGTH};
use std::collections::BTreeSet;

/// Why a form document cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum ValidateError {
    #[error("the form has an empty id")]
    EmptyFormId,
    #[error("the form has {0} pages, over the cap of {MAX_PAGES}")]
    TooManyPages(usize),
    #[error("the form has {0} fields, over the cap of {MAX_FIELDS}")]
    TooManyFields(usize),
    #[error("a page has an empty id")]
    EmptyPageId,
    #[error("two pages share the id {0:?}")]
    DuplicatePageId(String),
    #[error("a field has an empty id")]
    EmptyFieldId,
    #[error("two fields share the id {0:?}")]
    DuplicateFieldId(String),
    #[error("field {0:?} is a statement, so it cannot be required")]
    RequiredStatement(String),
    #[error("field {0:?} has {1} options, over the cap of {MAX_OPTIONS}")]
    TooManyOptions(String, usize),
    #[error("field {0:?} has no options")]
    NoOptions(String),
    #[error("field {0:?} has an option with an empty id")]
    EmptyOptionId(String),
    #[error("field {0:?} repeats the option id {1:?}")]
    DuplicateOptionId(String, String),
    #[error("field {0:?} has a rating max of 0; it must be at least 1")]
    ZeroRating(String),
    #[error("field {0:?} has min above max")]
    MinAboveMax(String),
    #[error("field {0:?} caps text at {1} bytes, over the cap of {MAX_TEXT_LENGTH}")]
    TextCapTooLarge(String, u32),
    #[error("field {0:?} caps text at 0 bytes")]
    ZeroTextCap(String),
}

/// Checks ids, caps and per-kind invariants. A form that passes can be rendered, bound to a
/// design and given responses.
pub fn validate(form: &Form) -> Result<(), ValidateError> {
    if form.id.is_empty() {
        return Err(ValidateError::EmptyFormId);
    }
    if form.pages.len() > MAX_PAGES {
        return Err(ValidateError::TooManyPages(form.pages.len()));
    }
    let field_count = form.fields().count();
    if field_count > MAX_FIELDS {
        return Err(ValidateError::TooManyFields(field_count));
    }
    let mut page_ids = BTreeSet::new();
    for page in &form.pages {
        if page.id.is_empty() {
            return Err(ValidateError::EmptyPageId);
        }
        if !page_ids.insert(page.id.as_str()) {
            return Err(ValidateError::DuplicatePageId(page.id.clone()));
        }
    }
    let mut field_ids = BTreeSet::new();
    for field in form.fields() {
        if field.id.is_empty() {
            return Err(ValidateError::EmptyFieldId);
        }
        if !field_ids.insert(field.id.as_str()) {
            return Err(ValidateError::DuplicateFieldId(field.id.clone()));
        }
        check_field(field)?;
    }
    Ok(())
}

fn check_field(field: &Field) -> Result<(), ValidateError> {
    match &field.kind {
        FieldKind::Statement if field.required => Err(ValidateError::RequiredStatement(field.id.clone())),
        FieldKind::Choice { options, .. } => check_options(field, options),
        FieldKind::Rating { max: 0 } => Err(ValidateError::ZeroRating(field.id.clone())),
        FieldKind::Number { min: Some(min), max: Some(max), .. } if min > max => Err(ValidateError::MinAboveMax(field.id.clone())),
        FieldKind::Text { max_length: Some(0), .. } => Err(ValidateError::ZeroTextCap(field.id.clone())),
        FieldKind::Text { max_length: Some(cap), .. } if *cap as usize > MAX_TEXT_LENGTH => {
            Err(ValidateError::TextCapTooLarge(field.id.clone(), *cap))
        }
        _ => Ok(()),
    }
}

fn check_options(field: &Field, options: &[crate::ChoiceOption]) -> Result<(), ValidateError> {
    if options.is_empty() {
        return Err(ValidateError::NoOptions(field.id.clone()));
    }
    if options.len() > MAX_OPTIONS {
        return Err(ValidateError::TooManyOptions(field.id.clone(), options.len()));
    }
    let mut ids = BTreeSet::new();
    for option in options {
        if option.id.is_empty() {
            return Err(ValidateError::EmptyOptionId(field.id.clone()));
        }
        if !ids.insert(option.id.as_str()) {
            return Err(ValidateError::DuplicateOptionId(field.id.clone(), option.id.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ValidateError, validate};
    use crate::FieldKind;
    use crate::tests::sample;

    #[test]
    fn accepts_the_sample() {
        validate(&sample()).unwrap();
    }

    #[test]
    fn rejects_duplicate_field_ids_across_pages() {
        let mut form = sample();
        form.pages[1].fields[0].id = "name".into();
        assert!(matches!(validate(&form), Err(ValidateError::DuplicateFieldId(_))));
    }

    #[test]
    fn rejects_required_statements() {
        let mut form = sample();
        form.pages[0].fields[0].kind = FieldKind::Statement;
        form.pages[0].fields[0].required = true;
        assert!(matches!(validate(&form), Err(ValidateError::RequiredStatement(_))));
    }

    #[test]
    fn rejects_bad_kind_invariants() {
        let mut form = sample();
        form.pages[0].fields[3].kind = FieldKind::Rating { max: 0 };
        assert!(matches!(validate(&form), Err(ValidateError::ZeroRating(_))));

        let mut form = sample();
        form.pages[0].fields[2].kind = FieldKind::Number { min: Some(5.0), max: Some(1.0), integer: false };
        assert!(matches!(validate(&form), Err(ValidateError::MinAboveMax(_))));

        let mut form = sample();
        form.pages[0].fields[4].kind = FieldKind::Choice { options: Vec::new(), multiple: false };
        assert!(matches!(validate(&form), Err(ValidateError::NoOptions(_))));
    }
}
