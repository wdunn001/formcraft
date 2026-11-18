# FormCraft

FormCraft is a form and survey builder for the Craft suite. A form is pages of fields; a
logic document adds conditional behavior (show, hide, require, jump); a binding document ties
the form to a FigCraft design. A bound form looks exactly like the design it came from.
FormCraft does form design on its own too: the form document stands alone and renders without
a design.

The repository is an early scaffold. It is built to the conventions of the Craft apps at
[github.com/storytold](https://github.com/storytold) and is written to merge into that suite.

## What exists

- `crates/form` holds the form document: pages, fields, response checking, caps and
  validation.
- `crates/logic` holds the rules: structured conditions over the answers, visibility and
  required overrides, page jumps. Later rules win.
- `crates/fig` holds the FigCraft binding: a sidecar document mapping design nodes to fields,
  pages and controls, with suggestions from layer names like `field:email`.
- `crates/state` is the headless form state, in the shape of TanStack Form: field values
  and meta (errors, touched, dirty, validating), the change/blur/submit lifecycle with
  sync checks before async ones, sequence-guarded async checks and logic-driven
  revalidation across fields.
- `apps/formcraft-cli` validates, previews, fills and exports forms.

## A form, filled

```sh
formcraft-cli new survey.json
formcraft-cli preview survey.json --logic survey.logic.json --answers so-far.json
formcraft-cli fill survey.json --logic survey.logic.json --answers answers.json --out response.json
formcraft-cli export survey.json response.json --out responses.csv
```

The formats are specified in [docs/form-format.md](docs/form-format.md) and
[docs/fig-binding.md](docs/fig-binding.md).

## Build

```sh
cargo xtask ci
```

The gate runs the format check, clippy with warnings denied, the test suite and the asset
attribution check. CI calls the same gate through the suite's shared workflow.

## License

Dual-licensed under the MIT License or the Apache License 2.0, at your option.
