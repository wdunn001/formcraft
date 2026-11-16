# Roadmap

Stage: scaffold, 0.1.0. The form, logic and binding documents exist with validation, response
checking and a CLI, all tested. Nothing ships yet. The ranked work list lives in
[docs/gaps.md](docs/gaps.md).

## Done

- Form document model: pages, fields, per-kind invariants, caps, format versioning.
- Response checking with every issue reported at once, including the logic overlay.
- Conditional logic: structured conditions, show/hide, require/relax, page jumps,
  later-rule-wins evaluation.
- FigCraft binding sidecar with validation and layer-name suggestions.
- CLI: `new`, `validate`, `preview`, `fill`, `fig-suggest`, `export`.

## Next

- Read FigCraft documents through the suite's `craft-fig` and `craft-figcraft-format` crates.
  The exported node listing then retires.
- Publish a form: responses need somewhere to land. CollabCraft is the natural store, beside
  FlowCraft's flow lane.
- A FlowCraft trigger: a submitted response should be able to start a flow.

## Later

- An egui form filler and builder (`apps/formcraft`), built like the suite's other apps.
- A browser build served at `craft.quasarke.net`. A form then fills from a link.
- E-signature fields tracking the suite's e-signature tooling as it lands.
