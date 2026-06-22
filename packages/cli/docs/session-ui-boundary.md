# Session and UI Boundary

The CLI composes Host and TUI but does not translate transcripts or reconcile
state. Its responsibilities are model/settings resolution, Host construction,
and TUI launch.

Session entry loading remains a Host API and entry-to-timeline conversion remains
a TUI concern. CLI modes that later emit JSON or RPC results must choose an
explicit schema: model transcript for execution output, or ID-bearing session
entries for durable state. They must not present a positional Message array as a
durable session snapshot.

