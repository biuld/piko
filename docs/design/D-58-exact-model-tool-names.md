# D-58: Exact model-visible tool names

> Status: accepted
> Implements: [F-03](../features/F-03-prompt-assembly.md)

## Goal

Prevent models from describing or invoking guessed tool aliases when the
structured run surface already provides an authoritative name. In particular,
an upstream search tool exposed as `web_search` must not be reported as
`search`, and the model must not claim companion tools that are absent.

## Design

Hostd keeps the structured tool catalog authoritative. The global-stable
`platform.policy` block explicitly requires exact names from the structured
tools supplied for the run and prohibits invented aliases or unavailable
tools. No tool definition, llmd capability kind, provider wire definition, or
execution route changes: `search` remains the protocol-neutral internal kind,
while `web_search` remains the model-visible and provider-wire name.

Changing the policy content changes its block and stable-prefix digests, so no
assembly-version bump is required.

## Verification

- Assert the frozen `platform.policy` contains the exact-name and no-invention
  requirements with the todo feature both disabled and enabled.
- Retain llmd/orchd tests that resolve upstream kind `search` to the
  model-visible name and Responses wire type `web_search`.
