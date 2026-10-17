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

## Licences (for the customer's question about legal use)

- EuroLLM-9B-Instruct-2512: Apache License 2.0 (model card).
- MiLMMT-46-4B: published under the Gemma licence (it is a fine-tune of Gemma 3
  4B), i.e. Google's Gemma Terms of Use, which permit commercial use subject to
  Google's Prohibited Use Policy and require passing the terms on with any
  redistributed model. Running it locally sends no text anywhere.

Neither is legal advice; a company should have its own counsel confirm.
