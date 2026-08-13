# ADR-016: Installed files are configuration authority

> Status: accepted
> Date: 2026-08-13

## Context

piko's editable TOML resources are currently embedded into multiple binaries.
Users can add overrides, but the shipped base catalog remains invisible and
immutable. Adding an installer creates an opportunity to establish one clear
authority for product configuration.

## Decision

The active user installation under `~/.piko` is the authority for global
settings and shipped catalogs. Production binaries do not embed editable
settings, agents, model catalogs, or themes. The installer materializes these
files and never overwrites an existing configuration during a normal reinstall.

Project-local configuration remains a higher-precedence overlay where already
supported. Runtime semantic safety defaults may remain in code, but they are
not a hidden copy of an editable catalog.

## Consequences

- Users can inspect, version, and edit the effective global configuration.
- Installation becomes a required packaging step for production binaries.
- A damaged or incomplete installation is visible instead of silently masked.
- Future catalog upgrades need an explicit merge/reset UX rather than implicit
  overwrites.
