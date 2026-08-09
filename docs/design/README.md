# Implementation designs

Design docs describe how a feature is built: data flow between packages,
responsibility boundaries, protocol types, state ownership, and key technical
decisions. Write a design doc after the feature PRD is agreed, before
implementing.

Create new documents from [`_TEMPLATE.md`](_TEMPLATE.md). Use stable
identifiers. A design's number matches the feature it implements: the F-06
tool-system design is `D-06-tool-dispatch.md`, and the F-01 turn-runtime
design is `D-01-turn-runtime.md`. Feature-local choices belong in the design;
decisions that affect multiple features or package boundaries belong in
[`../decisions/`](../decisions/).

## Recent designs

- [D-37: Native OpenAI-family model protocols](D-37-native-openai-model-protocols.md)
  implements F-25 and replaces genai with piko-owned Responses and Chat
  Completions adapters (implemented; V-37).
- [D-36: Provider authentication](D-36-provider-authentication.md) implements
  F-24 typed provider authentication and refresh.
