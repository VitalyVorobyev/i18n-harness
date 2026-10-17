# Benchmark: Slovak from German / English, two local models (2026-10-17)

Evidence for choosing a model for a real job: nine Qt Linguist catalogs
(6,947 strings, 5,236 distinct) labelled `sk_SK` whose targets were pre-filled
with German, to be translated into Slovak. The catalogs are private; this page
records the method and the aggregate results only.

## Method

- `harness-lab bench <folder> --pivot de_DE --sample 200 --seed 42` with four
  configurations: two models × two inputs.
  - `@auto`: the existing German text when it is sound (same placeholders and
    markup as the English original), otherwise the English original.
  - `@source`: always the English original.
- Sample: seeded and stratified (markup, placeholders, identifiers, strings with
  no German, long, short, plural), de-duplicated by input text: 170 strings.
- Models via Ollama (Q8_0 GGUF), prompt profile chosen from what the server
  reports: MiLMMT-46-4B (pass-through template → `completion` profile), EuroLLM-9B-Instruct-2512 (chat template →
  `chat` profile). Greedy decoding, temperature 0, seed 42. Apple silicon laptop.
- Quality: every candidate scored 1–5 by a blind judge (Claude) for meaning,
  natural Slovak UI style, and preserved placeholders, markup and identifiers;
  configuration names hidden behind shuffled letters, identical outputs scored
  once, key applied after scoring (`harness-lab score`).
- Structure: the deterministic gate on every output.

## Results

| Configuration | Mean score | Best or tied-best (of 170) | Hard gate errors | Median s / string |
|---|---|---|---|---|
| MiLMMT-46-4B, English input | **4.24** | **104** | 0 | 0.29 |
| MiLMMT-46-4B, German input | 3.97 | 83 | 0 | 0.28 |
| EuroLLM-9B, English input | 3.56 | 71 | 7 | 0.48 |
| EuroLLM-9B, German input | 3.50 | 61 | 5 | 0.54 |

Mean score by kind of string:

| Stratum | MiLMMT en | MiLMMT de | EuroLLM en | EuroLLM de |
|---|---|---|---|---|
| markup | 4.23 | 4.40 | 3.27 | 3.73 |
| placeholders | 4.43 | 3.98 | 3.93 | 3.53 |
| identifiers | 4.05 | 3.75 | 3.05 | 2.65 |
| long | 4.05 | 3.40 | 3.25 | 3.35 |
| short | 4.17 | 3.72 | 3.93 | 3.86 |
| no German text | 4.33 | 4.33 | 3.60 | 3.60 |

## Findings

- **MiLMMT-46-4B is the better model for this pair** at half the time per
  string. It keeps markup and identifiers far more reliably; EuroLLM drops or
  rewrites tags (its hard errors), translates identifiers (`PageViewParam`,
  `scope_id`) and sometimes slips into Czech forms.
- **The English original is a better input than the existing German.** The
  German has its own mistakes (a sorting method "Triangle" rendered
  "Dreiecksraupe", "Get number of repairables" rendered "a certain number of
  repairs"), which a translation from German faithfully carries over. The gap is
  largest on long strings and placeholders.
- Both models need the gate: lost markup, a translated identifier or a dropped
  plural number are caught and never written.
- A runaway generation (a chat model looping until the request timed out) made
  the harness cap each answer's length; see `ModelBackend`.

## Follow-up: a cloud model, Claude Haiku 5.5 (2026-10-17)

The same 170 strings, English input, translated by Claude Haiku 5.5 (an
Anthropic cloud model) with the rules every chat model gets (see the `chat`
prompt profile), plus each string's code context and developer comment.
Scored blind against MiLMMT-46-4B (English input) by the same judge, two
candidates per string behind shuffled letters, identical outputs scored once
(52 of 170 were identical); the deterministic gate checked both through the
session, as the app does.

| Configuration | Mean score | Score 4–5 | Score 1–2 (wrong meaning or unusable) | Hard gate errors | Warnings |
|---|---|---|---|---|---|
| Claude Haiku 5.5 | **4.79** | **99%** | **0%** | 0 | 15 |
| MiLMMT-46-4B, English input | 4.04 | 74% | 12% | 0 | 27 |

Haiku scored higher on 72 strings, lower on 10, the same on 88. It led in
every kind of string (identifiers 4.70 vs 3.80, markup 4.70 vs 3.83, long
4.85 vs 3.95). Its 10 losses are minor: lower-case labels where the
original capitalises (`plocha` for *Area*), *Prah* for *Threshold*, keeping
a camelCase key that the customer's German translates (the instruction to
keep camelCase identifiers caused that). MiLMMT's wrong-meaning errors that
Haiku avoided include *Circularity* → *Kruhové hospodárstvo* (circular
economy), *Open Chat* → *Otvorený chat* (an opened chat), *Filter tickets* →
*Filtrovať vstupenky* (event tickets), *Role Name* → *Názov úlohy* (task) and
Czech words (*úkol*).

Caveats:

- The judge (Claude) belongs to the same model family as the candidate;
  self-preference cannot be ruled out. A native Slovak reviewer's blind
  check is the stronger evidence.
- Judge consistency: MiLMMT's identical outputs scored 4.24 in the first
  round (four candidates) and 4.04 here (next to a stronger candidate), so
  scores move by about ±0.2 between rounds; the 0.75 gap is well beyond that.
- Haiku saw each string's context and developer comment; MiLMMT, a
  completion model, cannot take them. This compares the two as the app runs
  them, not the bare models.
- Haiku translated the 170 strings in about a minute. The text is sent to
  Anthropic's API, so it needs the customer's consent; MiLMMT runs offline.

## Licences (for the customer's question about legal use)

- EuroLLM-9B-Instruct-2512: Apache License 2.0 (model card).
- MiLMMT-46-4B: published under the Gemma licence (it is a fine-tune of Gemma 3
  4B), i.e. Google's Gemma Terms of Use, which permit commercial use subject to
  Google's Prohibited Use Policy and require passing the terms on with any
  redistributed model. Running it locally sends no text anywhere.

Neither is legal advice; a company should have its own counsel confirm.
