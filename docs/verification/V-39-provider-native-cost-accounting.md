# V-39: Provider-native cost accounting acceptance evidence

> Date: 2026-08-12
> Verifies: F-28 / D-40 / ADR-013
> Environment: macOS; local fixtures and stub servers; official pricing pages
> Status: passed

## Reproduction

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The workspace test suite passes with loopback access enabled for OAuth callback
and gateway stub-server tests. Workspace clippy passes with warnings denied.

## Catalog and target evidence

- OpenAI GPT-5.6 schedules resolve by API surface: API-key/platform targets
  carry USD `list_price`; OAuth/subscription targets copy the same schedule as
  USD `api_equivalent`.
- GPT-5.6 catalog fixtures include cache-write prices and the over-272K input
  threshold tier with separate input/output multipliers.
- DeepSeek V4 Flash resolves CNY rates of ¥1 cache miss, ¥0.02 cache hit, and
  ¥2 output per million tokens. V4 Pro resolves ¥3, ¥0.025, and ¥6.
- Existing GPT-4o and GPT-4o mini schedules were migrated from middleware into
  the OpenAI catalog, so the architecture change does not drop their prior
  estimates.
- Loader validation covers surface references, schedule copies, currency
  shape, non-negative prices, and positive tier multipliers.

## Usage and calculation evidence

- Chat Completions fixtures prove DeepSeek `prompt_cache_hit_tokens` maps to
  semantic cache-read usage while total prompt tokens remain the input basis.
- Calculator tests cover uncached input, cache read, explicit cache write,
  output, threshold tiers, and CNY output.
- The calculator receives a resolved schedule and contains no provider/model
  price switch.
- Missing schedules leave the cost ledger empty rather than creating a zero
  amount.

## Session, client, and telemetry evidence

- Protocol accumulation tests merge matching currency/basis entries and retain
  a separate CNY entry when a session already contains USD.
- The TUI formatting fixture renders mixed entries as `~$0.0042 + ¥0.42`,
  visibly distinguishing API-equivalent OAuth consumption.
- Hostd telemetry emits generic model/turn cost counters with currency and
  basis attributes, while token counters remain provider-neutral.
- No compatibility decoder for the former scalar cost payload exists, matching
  the schema-v3 no-migration policy.

## Source verification

- OpenAI GPT-5.6 model pages and latest-model pricing guidance were fetched
  from official OpenAI documentation on 2026-08-12.
- DeepSeek's Chinese Models & Pricing page was fetched on 2026-08-12 and is the
  source of the CNY schedules and cache-hit/cache-miss distinction.
