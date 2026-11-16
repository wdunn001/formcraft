# The FigCraft binding

A form designed in FigCraft runs through a binding document: a sidecar next to the design
that maps design nodes to form fields, pages and controls. The design file stays FigCraft's
alone; FormCraft never writes into it.

```json
{
  "format_version": 1,
  "fig_file": "contact.fig",
  "form_file": "contact.form.json",
  "bindings": [
    {"node_id": "1:2", "node_name": "field:email", "target": {"type": "field", "field": "email"}},
    {"node_id": "1:4", "node_name": "submit", "target": {"type": "submit"}}
  ]
}
```

Targets: `field` (the input for a field), `page` (the frame for a page), `submit` (the
control that submits), `error-text` (where a field's validation message shows). A node binds
once, a field has at most one input binding, a form has at most one submit.

## Suggestions by layer name

Designers opt in by naming layers: `field:email`, `page:p2`, `error:email`, `submit`.
`formcraft-cli fig-suggest --nodes nodes.json` reads an exported node listing (a JSON array
of `{"id", "name", "kind"}`) and writes a binding set from the names that resolve against the
form. Names that resolve to nothing are left alone. Decorative layers never bind.

## Where this is going

Reading the design directly belongs to the suite's `craft-fig` and `craft-figcraft-format`
crates in craft-libs; adopting them is the top entry in [gaps.md](gaps.md). The binding
format stays the same when that lands. It names nodes, never file offsets.
