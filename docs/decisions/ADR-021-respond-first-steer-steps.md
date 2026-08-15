# ADR-021: Steered user messages are answered before further tool work

> Status: accepted
> Date: 2026-08-16

## Context

F-01 steers a user message into a running turn at the next model-step
boundary, but places no obligation on the model to reply to it. In a
production session the user asked twice for a status report while the turn
ran; the model acknowledged both messages in its reasoning and kept calling
tools, and the turn eventually died on the context budget with no answer
delivered. Prompt-level guidance alone was demonstrably insufficient, so the
runtime needs a structural guarantee that a mid-turn message gets an answer.

## Decision

- After a steered user message is committed, the next model step is a
  **respond-only step**: tools are disabled for that step
  (`tool_choice = None` regardless of configuration) and the prompt carries an
  explicit instruction to answer the newly delivered message in text.
- The respond-only step commits its assistant message like any other step. If
  the provider still returns tool calls, the step fails closed instead of
  executing them.
- After the respond-only step, the turn resumes its normal loop with tools
  enabled. A steer redirects the running turn; it does not terminate it.

## Consequences

- A user interrupt can no longer be buried under an unbounded tool loop: the
  very next model output after the interrupt is a text reply.
- Turn lifecycle is preserved: steers still do not end or restart turns, and
  follow-up queueing/cancellation semantics (F-01) are untouched.
- The respond-only step costs one extra model step when a steer arrives near
  natural turn completion.
- Enforcement is structural but not absolute: a provider could in principle
  return an empty-text response, which the runtime cannot force into content.
