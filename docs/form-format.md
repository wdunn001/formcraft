# The form and logic file formats

A form is one JSON document; its conditional logic is a second document next to it. Both
carry `format_version` 1; a reader refuses newer versions and documents over 4 MiB.
`formcraft-cli validate` checks everything on this page.

## The form

```json
{
  "format_version": 1,
  "id": "visit-survey",
  "title": "Visit survey",
  "pages": [
    {"id": "p1", "title": "Your visit", "fields": [
      {"id": "rating", "label": "How was it?", "required": true, "kind": {"type": "rating", "max": 5}}
    ]}
  ]
}
```

Page ids and field ids are unique across the form. Unknown fields in any object are refused.

## Field kinds

- `text` with optional `multiline` and `max_length` (bytes, at most 65536).
- `number` with optional bounds (`min`, `max`) and an `integer` switch.
- `choice` with `options` (id and label pairs) and optional `multiple`.
- `date`, answered as `"YYYY-MM-DD"`.
- `email`, checked for shape only.
- `rating` with `max`; answers run from 1 to `max`.
- `signature`, answered as `{"name": "...", "signed_at": "RFC 3339, optional"}`.
- `statement`, display-only; it takes no answer and cannot be required.

## Responses

```json
{"form_id": "visit-survey", "answers": {"rating": 5, "email": "ana@example.com"}}
```

Checking returns every issue at once: missing required answers, wrong shapes, unknown fields,
answers on hidden fields. An empty string or an empty list counts as no answer.

## The logic document

```json
{
  "format_version": 1,
  "form_id": "visit-survey",
  "rules": [
    {"id": "unhappy-must-explain",
     "when": {"type": "less-than", "field": "rating", "value": 3.0},
     "actions": [{"type": "require", "field": "comment"}]}
  ]
}
```

Conditions are structured, with no expression strings: `answered`, `equals`, `not-equals`,
`greater-than`, `less-than`, `contains`, `all`, `any`, `not`. A missing or wrongly shaped
answer makes a comparison false. Most fields have no answer yet while a form is being
filled.

Actions: `show-field`, `hide-field`, `show-page`, `hide-page`, `require`, `relax`,
`jump-to-page`. Rules evaluate in order; where two rules disagree, the later one wins.

## Caps

Form and logic files are hostile input. The caps: 256 pages, 4096 fields, 1024 options per
choice, 4096 rules, condition nesting 64, text answers 64 KiB, documents 4 MiB.
