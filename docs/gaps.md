# Gaps

The ranked work list. Take work from the top.

1. The egui filler (`apps/formcraft`) over `formcraft-state`: render a form page by page,
   apply logic live, show field meta (errors after touch, validating), submit. Built like
   the suite's other apps on the storytold egui pins.
2. Definition structure per dynamic-forms: `group` (nested object), `array` (repeating
   group of one structure), layout elements (section, tabs, markdown) and `action`
   (buttons wired to registered handlers). Field kinds move behind a registry so custom
   kinds declare id, value shape and validators without the schema ever carrying code.
3. Response delivery. Responses persist through the cloud seam (`craft_cloud::Files`);
   external respondents get keyed links the CollabCraft service already uses for invites;
   submission emits `form-submitted`, FlowCraft's trigger, with the response as payload.
4. Signature placements: the `signature` kind gains an optional normalized-page rectangle
   so a bound form emits `craft-esign` `FieldPlacement` values directly (the kind strings
   already align).
5. Adopt the suite's design-file crates. `crates/fig` reads an exported node listing
   today; it should read FigCraft documents through `craft-fig` and
   `craft-figcraft-format` from craft-libs, binding to boards node ids.
6. A FigCraft export path for the node listing, over the control port. `fig-suggest` then
   needs no manual export.
7. The form builder in the same app: edit pages, fields and rules with the structured
   condition model. No expression parser is ever needed.
8. Async validator hosts: a CollabCraft-backed uniqueness/lookup check wired through
   `formcraft-state`'s pending-check queue, with the debounce hint applied.
9. Localization of labels and validation messages once the suite's pattern for it lands.
10. A browser build served from CollabCraft. A form is then fillable from a link.
