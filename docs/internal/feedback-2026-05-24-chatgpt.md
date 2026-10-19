The core idea is solid:

You are not building “just an LLM translator.” You are building a technical translation workbench with:

1. Structured translation units from Qt Linguist files: .ts, maybe .qm, .xlf later.
2. Project context: languages, input files, glossary, reference translations, style rules.
3. Local translation execution: Gemma or similar model does the main work.
4. Human verification loop: translator edits, approves, rejects, comments.
5. Evaluation loop: curated reference set produces quality metrics.
6. Prompt improvement loop: a stronger coding/agent model analyzes failures and updates translation prompts, glossary handling, examples, or rules.

That is a very good product shape. The important insight is that the human-corrected translations become training/evaluation material, but instead of fine-tuning immediately, you improve prompts, rules, few-shot examples, and QA checks.

What I think you may be missing

1. Translation unit state model

Each entry should have a clear lifecycle, not just “translated/not translated.”

For example:

new
machine_translated
needs_review
reviewed
approved
locked
rejected
obsolete
conflict

You will want this because technical translators need to filter by state:

* show only untranslated strings
* show only changed source strings
* show only strings where LLM confidence is low
* show only strings affected by glossary updates
* show only strings where source text changed since last approval

Without this, the UI will quickly become messy.

2. Source change tracking

Qt .ts files change over time. A source string may be edited by developers after translation.

You need to detect:

* new source strings
* deleted/obsolete strings
* changed source strings
* same source text but moved context
* same source text with different meaning in different UI contexts

For technical translation this matters a lot. "Open" can mean open file, open valve, open dialog, open connection.

So every translation unit should probably have a stable internal ID, plus a hash of:

context + source + disambiguation/comment + location

Do not rely only on the raw source text.

3. Context is king

Qt Linguist files often contain useful metadata: context name, source location, developer comments, old source strings, plural forms.

The local LLM should not receive just:

Translate: "Start"

It should receive something like:

Application domain: industrial machine vision
UI context: CameraConnectionDialog
Developer comment: Button that starts image acquisition
Source: Start
Target language: German
Glossary:
- acquisition = Aufnahme / Bildaufnahme, not Akquisition
Style:
- concise UI label
- imperative verb where appropriate

Your app should make context visible in the UI and pass it into the prompt.

4. Glossary enforcement should be testable

A glossary should not be passive text. It should be used in three ways:

1. Prompt context: tell the model preferred terms.
2. Post-translation QA: check whether required terms were used.
3. Conflict detection: detect if the same source term is translated inconsistently.

Glossary entries should support more than just source → target:

source term
target term per language
forbidden translations
part of speech
domain
comment/rationale
case sensitivity
inflection policy
examples

For example:

"acquisition"
German preferred: "Bildaufnahme"
German forbidden: "Akquisition"
Comment: In this UI it means camera image capture, not business acquisition.

5. Plural forms are a trap

Qt .ts files can contain plural messages. These need special handling because languages have different plural rules.

Do not treat plural strings as ordinary entries. The UI needs to expose plural variants explicitly.

Example:

%n image(s) acquired

German may be simple, but Slavic languages are not. The LLM needs language-specific plural instructions, and your data model should preserve Qt plural structure exactly.

6. Placeholder and markup validation

This is essential.

Technical translation strings often contain:

%1
%2
{count}
{name}
\n
\t
&amp;
<source>
<b>text</b>
Ctrl+S

The app should automatically validate:

* all placeholders are preserved
* no extra placeholders are introduced
* XML/HTML markup remains valid
* keyboard accelerators are preserved or adapted
* newline structure is preserved where required
* punctuation style matches target language
* Qt mnemonic markers like &File are handled correctly

This should be one of your strongest built-in QA features.

7. Translation memory, not only curated references

You described curated reference translations, which is good. But you probably also need a broader translation memory generated from approved project translations.

Two different things:

Curated references
Small, high-quality examples used for prompt/evaluation.

Translation memory
Large database of previously approved translations used for retrieval.

When translating a new entry, retrieve similar previous source strings and feed them into the prompt.

Example:

Source: "Camera disconnected"
Similar approved translations:
- "Sensor disconnected" → "Sensor getrennt"
- "Camera connection lost" → "Kameraverbindung verloren"

This will improve consistency a lot.

8. Segment-level comments and rationale

Allow reviewers to attach notes:

"Use 'Bildaufnahme' here because the customer documentation uses that term."

These notes should later become input for the prompt-improvement agent.

This is very valuable because human corrections alone tell the agent what changed, but comments explain why.

9. Prompt versioning

Every machine translation should record:

model
model version
prompt template version
glossary version
reference set version
temperature/settings
timestamp

Otherwise you cannot evaluate improvements honestly.

You want to compare:

Prompt v7 + glossary v3
vs
Prompt v8 + glossary v3

Not just “the model seems better now.”

10. A proper evaluation report

Your metrics report should not only use generic BLEU-like metrics. For technical translation, I would include:

* placeholder preservation pass/fail
* glossary compliance
* exact match against reference where applicable
* edit distance from reference
* human edit distance from machine proposal
* number of reviewer changes
* repeated inconsistency count
* untranslated source leakage
* forbidden term usage
* length expansion warnings for UI labels
* accelerator/mnemonic conflicts
* plural-form completeness
* XML validity

LLM-as-judge can be useful, but do not make it the primary metric. Deterministic checks are more trustworthy.

Features I would strongly consider

Project structure

A project could look like this:

translation-project/
  project.yaml
  sources/
    app_de.ts
    app_fr.ts
  glossary/
    glossary.yaml
  references/
    curated_de.yaml
    curated_fr.yaml
  memory/
    approved.sqlite
  prompts/
    translate.md
    review.md
    repair.md
  reports/
    eval-2026-05-24.html
  snapshots/
    ...

Or use one SQLite database plus exported files. I would still keep project files human-readable where possible.

Three LLM actions, not one

Instead of only “translate,” define separate actions:

1. Translate
    Produces initial translation.
2. Review
    Checks an existing translation against source, glossary, placeholders, style.
3. Repair
    Fixes a specific failed check.

This gives you better control than one huge prompt.

Example workflow:

Translate entry
→ deterministic QA
→ if failed, run repair prompt
→ deterministic QA again
→ mark needs_review or machine_translated

Batch mode with review queue

The UI should support:

* translate selected entries
* translate all untranslated entries
* stop/resume batch
* show failed entries
* show entries needing review
* approve with keyboard shortcut
* jump to next issue

Translators will care about throughput.

Diff view

For each entry, show:

source
previous translation
machine proposal
current edited translation
reference translation, if available

A good diff UI is probably more useful than a fancy chat interface.

Per-language style guide

Glossary is about terms. Style guide is about writing style.

Examples:

German:
- Prefer formal "Sie" in user-facing messages.
- Use concise button labels.
- Avoid English loanwords unless in glossary.
- Use sentence case for menu items.
- Preserve product names.

This should be editable in the project.

“Do not translate” list

You need a protected-token system:

Product names
API names
file extensions
command names
environment variables
CLI flags
keyboard shortcuts
error codes

Example:

Do not translate:
- GenICam
- GigE Vision
- calib-targets
- --config
- .json
- TCP

UI screenshot/context attachment

Eventually, a killer feature would be screenshot context.

A translator sees "Apply" and can attach or view the UI screenshot/dialog where the string appears.

Even if you do not implement this early, design the data model so a translation unit can have references to:

source file location
screenshot
component/page/dialog
developer note

Architecture suggestion

Tauri backend responsibilities

Use Rust/Tauri for:

* file parsing/writing
* Qt .ts XML handling
* SQLite database
* project snapshots
* deterministic QA checks
* invoking local LLM backend
* long-running batch jobs
* filesystem safety
* import/export

React frontend responsibilities

Use React for:

* project dashboard
* translation table
* entry editor
* glossary editor
* reference set editor
* batch progress
* QA report viewer
* diff view
* filters/search

Local model integration

Do not hardcode Gemma directly. Define a provider abstraction:

LocalModelProvider
  - Ollama
  - LM Studio
  - llama.cpp server
  - custom command

This gives you flexibility. Corporate environments often have weird restrictions, so users may need different local model runners.

Agentic prompt-improvement loop

This part is interesting. I would structure it as a repeatable “skill” that consumes artifacts:

inputs:
  project.yaml
  glossary.yaml
  style-guide.md
  prompt templates
  curated references
  failed translations
  human edits
  QA reports
outputs:
  updated prompt templates
  suggested glossary changes
  suggested style guide changes
  evaluation report
  changelog explaining what changed

Important: the agent should not silently rewrite everything. It should produce a patch and rationale.

Possible MVP

For a strong first MVP, I would keep it narrow:

1. Import Qt .ts files.
2. Show entries in a table.
3. Support one source language and one target language first.
4. Edit and save translations.
5. Run local LLM translation for selected entries.
6. Preserve placeholders and XML.
7. Add glossary.
8. Add curated reference examples.
9. Add deterministic QA report.
10. Export updated .ts.

Do not start with many file formats. Qt Linguist alone is already useful.

Things I would not overbuild early

I would postpone:

* fine-tuning
* many CAT file formats
* cloud collaboration
* user accounts
* complex permissions
* vector database infrastructure
* fully automatic prompt optimization
* beautiful analytics dashboards

The core risk is not UI beauty. The core risk is whether the system can produce consistent, reviewable, technically correct translations and improve from human corrections.

My honest take

This is a good project because it has a clear niche: technical translators working on structured software localization files.

The strongest design angle is:

Human translators remain in control, local LLMs do the repetitive draft work, deterministic QA catches mechanical errors, and a stronger agent improves the translation process from verified corrections.

That is much better than “chatbot translates strings.”

The most important missing pieces are probably:

1. robust translation-unit state tracking,
2. placeholder/markup/plural validation,
3. translation memory from approved strings,
4. prompt/glossary/reference versioning,
5. source-change detection,
6. a serious review queue,
7. deterministic quality metrics before LLM judging.

Get those right, and the product will feel practical rather than like a toy.