# Gaps

The ranked work list. Take work from the top.

1. Adopt the suite's design-file crates. `crates/fig` reads an exported node listing today;
   it should read FigCraft documents through `craft-fig` and `craft-figcraft-format` from
   craft-libs, pinned the way FigCraft pins them.
2. Response delivery. A filled form needs somewhere to land: a CollabCraft endpoint beside
   FlowCraft's draft flow lane. Responses then store, notify and can start a flow.
3. A FigCraft export path. FigCraft should be able to export the node listing (or FormCraft
   should read it over the control port) so `fig-suggest` needs no manual export.
4. An egui form filler (`apps/formcraft`): render a bound form, apply logic live, validate on
   the way, submit. Build it like the suite's other apps, with `craft-ui` where it fits.
5. A form builder in the same app: edit pages, fields and rules with the structured condition
   model. No expression parser is ever needed.
6. E-signature fields. The `signature` kind should track the suite's e-signature tooling as
   its API settles, including signer identity from the CollabCraft session.
7. A browser build served from CollabCraft. A form is then fillable from a link.
8. Localization of labels and validation messages once the suite's pattern for it lands.
9. Suite merge chores: `contributors/people.toml`, packaging, release workflow and the rest
   of the app-repo furniture, when the repository moves into the storytold org.
