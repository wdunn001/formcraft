# Roadmap

Stage: M1 of the program plan, 0.1.x. The form, logic and binding documents exist with
validation, response checking, a headless form state and a CLI, all tested. Nothing ships
yet. The ranked work list lives in [docs/gaps.md](docs/gaps.md).

## Done

- Form document model: pages, fields, per-kind invariants, caps, format versioning.
- Response checking with every issue reported at once, including the logic overlay.
- Conditional logic: structured conditions, show/hide, require/relax, page jumps,
  later-rule-wins evaluation.
- FigCraft binding sidecar with validation and layer-name suggestions.
- `crates/state`: the headless form state. Field values and meta, the change/blur/submit
  validation lifecycle with sync checks before async ones, sequence-guarded async checks
  and logic-driven revalidation across fields.
- CLI: `new`, `validate`, `preview`, `fill`, `fig-suggest`, `export`.

## M1, the rest

- The egui filler (`apps/formcraft`) over `formcraft-state`, applying logic live.

## M2

- Definition structure per dynamic-forms: groups, repeating arrays, layout elements and
  actions, with field kinds behind a registry.
- Response delivery: responses through the cloud seam, keyed links for external
  respondents and the `form-submitted` event that triggers a FlowCraft run.

## M3

- Signature fields carry normalized-page placements and emit `craft-esign` field
  placements directly, for the e-sign contract workflow.

## Later

- The form builder in the same app; reading FigCraft designs through the suite's
  `craft-fig` crates; a browser build served from CollabCraft.
