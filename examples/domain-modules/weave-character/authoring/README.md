# Guided Character authoring fixture

This checked corpus is original MIT-licensed Weave material. Its fictional identity, birth date,
inner life, voice direction, behavior prompts, fixed-point weights, conflict rule, rationales, and
expected outputs were authored for this repository. The fixture corpus depends only on the
repository's published Character contracts.

The fixture demonstrates one complete source-first workflow:

1. `workspace.empty.authoring-workspace` is a project-owned workspace with no account, commerce,
   or entitlement state.
2. `blank.authoring-overlay` creates Lumen Reed with an explicit full birth date, an inner-life
   value, a voice direction, and no personality defaults.
3. `lantern_choices.questionnaire-pack` declares 24 original behavior prompts, one traceable
   fixed-point facet weight per prompt, limitations, provenance, and an explicit cross-axis
   narrative-tension rule. It is a fiction-writing aid, not a psychometric instrument.
4. Answers produce a deterministic proposal with per-facet confidence and a complete trace.
   Every facet and conflict receives an explicit review decision before the receipt can change the
   sparse overlay.
5. `questionnaire.authoring-preview` shows base/effective values, the canonical changes, and the
   derived OCEAN invalidation before `workspace.revised` is persisted.
6. `workspace.reviewed` saves the final canonical/derived/alignment/context/provenance/diagnostic
   summary, and `exported.character` is the exact reviewed profile.

The additional fixtures cover a reviewed template-release migration, an alignment view becoming
blocking and stale after a canonical facet revision, a needs-changes final review, and a final
summary containing both reviewed alignment and accepted date context. The `invalid` directory
proves that stale draft revisions, unknown placeholders, unknown fields, broken template
coordinates, and direct modification of pack-owned extensions fail before persistence.

Regenerate or verify every JSON/RON pair and all nine schemas with:

```bash
cargo run -p weave-character --example guided_authoring_fixture -- --write
cargo run -p weave-character --example guided_authoring_fixture -- --check
```

The text workflow uses the same library functions as the editor:

```bash
cargo run -p weave-character -- authoring-preview \
  examples/domain-modules/weave-character/authoring/workspace.created.authoring-workspace.json \
  examples/domain-modules/weave-character/authoring/questionnaire.authoring-revision.json \
  --output target/lumen.authoring-preview.json

cargo run -p weave-character -- authoring-revise \
  examples/domain-modules/weave-character/authoring/workspace.created.authoring-workspace.json \
  examples/domain-modules/weave-character/authoring/questionnaire.authoring-revision.json \
  --output target/lumen.authoring-workspace.json
```
