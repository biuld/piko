# piko-protocol

The core Domain-Specific Language (DSL) and event definitions for Piko.

It defines `HostEvent` and `HostCommand` which form the event-sourcing backbone of the Piko architecture. 
By isolating these types into a standalone crate, Piko ensures that both the orchestrator (`orchd`) and the host (`hostd`) share a ubiquitous language without circular dependencies or boundary erosion.
