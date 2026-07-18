use crate::{
    BroadcastContract, CancellationMeaning, CapacityPolicy, ClosureMeaning, CommunicationKind,
    CommunicationScope, CommunicationSpec, DeliveryGuarantee, LatestContract, MailboxContract,
    OverflowPolicy, ReplyContract, ThreadBridgeContract,
};

macro_rules! contract {
    ($name:ident, $trait:ident, $spec:ident, $value:expr) => {
        pub struct $name;
        pub const $spec: CommunicationSpec = $value;
        impl $trait for $name {
            const SPEC: &'static CommunicationSpec = &$spec;
        }
    };
}

pub mod contracts {
    use super::*;

    contract!(
        AgentCommands,
        MailboxContract,
        AGENT_COMMANDS,
        CommunicationSpec {
            id: "orchd.agent.commands",
            kind: CommunicationKind::Mailbox,
            owner: "AgentActor",
            producers: &["AgentRuntime", "ExecutionTerminalWaiter"],
            consumer: "AgentActor",
            scope: CommunicationScope::Agent,
            delivery: DeliveryGuarantee::InMemory,
            capacity: CapacityPolicy::Bounded(32),
            overflow: OverflowPolicy::RejectOverload,
            closure: ClosureMeaning::RuntimeUnavailable,
            cancellation: CancellationMeaning::ExplicitCommand,
        }
    );
    contract!(
        AgentCommandReply,
        ReplyContract,
        AGENT_COMMAND_REPLY,
        CommunicationSpec {
            id: "orchd.agent.command_reply",
            kind: CommunicationKind::Reply,
            owner: "AgentRuntimeCaller",
            producers: &["AgentActor"],
            consumer: "AgentRuntimeCaller",
            scope: CommunicationScope::Request,
            delivery: DeliveryGuarantee::InMemory,
            capacity: CapacityPolicy::One,
            overflow: OverflowPolicy::NotApplicable,
            closure: ClosureMeaning::RuntimeUnavailable,
            cancellation: CancellationMeaning::DropInterestOnly,
        }
    );
    contract!(
        AgentRunStarted,
        ReplyContract,
        AGENT_RUN_STARTED,
        CommunicationSpec {
            id: "orchd.agent.run_started",
            kind: CommunicationKind::Reply,
            owner: "AgentRunAcceptance",
            producers: &["AgentActor"],
            consumer: "AgentRunAcceptance",
            scope: CommunicationScope::Request,
            delivery: DeliveryGuarantee::InMemory,
            capacity: CapacityPolicy::One,
            overflow: OverflowPolicy::NotApplicable,
            closure: ClosureMeaning::RuntimeUnavailable,
            cancellation: CancellationMeaning::DropInterestOnly,
        }
    );
    contract!(
        AgentRunReport,
        ReplyContract,
        AGENT_RUN_REPORT,
        CommunicationSpec {
            id: "orchd.agent.run_report",
            kind: CommunicationKind::Reply,
            owner: "AgentRunAcceptance",
            producers: &["AgentActor"],
            consumer: "AgentRunAcceptance",
            scope: CommunicationScope::Request,
            delivery: DeliveryGuarantee::InMemory,
            capacity: CapacityPolicy::One,
            overflow: OverflowPolicy::NotApplicable,
            closure: ClosureMeaning::RuntimeUnavailable,
            cancellation: CancellationMeaning::DropInterestOnly,
        }
    );
    contract!(
        AgentSnapshot,
        LatestContract,
        AGENT_SNAPSHOT,
        CommunicationSpec {
            id: "orchd.agent.snapshot",
            kind: CommunicationKind::LatestState,
            owner: "AgentActor",
            producers: &["AgentActor"],
            consumer: "AgentRuntime",
            scope: CommunicationScope::Agent,
            delivery: DeliveryGuarantee::LatestOnly,
            capacity: CapacityPolicy::Latest,
            overflow: OverflowPolicy::ReplaceLatest,
            closure: ClosureMeaning::RuntimeUnavailable,
            cancellation: CancellationMeaning::ScopeShutdown,
        }
    );
    contract!(
        ExecutionCommands,
        MailboxContract,
        EXECUTION_COMMANDS,
        CommunicationSpec {
            id: "orchd.execution.commands",
            kind: CommunicationKind::Mailbox,
            owner: "ExecutionActor",
            producers: &["AgentExecutionRuntime"],
            consumer: "ExecutionActor",
            scope: CommunicationScope::Execution,
            delivery: DeliveryGuarantee::InMemory,
            capacity: CapacityPolicy::Bounded(32),
            overflow: OverflowPolicy::RejectOverload,
            closure: ClosureMeaning::RuntimeUnavailable,
            cancellation: CancellationMeaning::ExplicitCommand,
        }
    );
    contract!(
        ExecutionCommandReply,
        ReplyContract,
        EXECUTION_COMMAND_REPLY,
        CommunicationSpec {
            id: "orchd.execution.command_reply",
            kind: CommunicationKind::Reply,
            owner: "AgentExecutionRuntime",
            producers: &["ExecutionActor"],
            consumer: "AgentExecutionRuntime",
            scope: CommunicationScope::Request,
            delivery: DeliveryGuarantee::InMemory,
            capacity: CapacityPolicy::One,
            overflow: OverflowPolicy::NotApplicable,
            closure: ClosureMeaning::RuntimeUnavailable,
            cancellation: CancellationMeaning::DropInterestOnly,
        }
    );
    contract!(
        ExecutionTerminal,
        ReplyContract,
        EXECUTION_TERMINAL,
        CommunicationSpec {
            id: "orchd.execution.terminal",
            kind: CommunicationKind::Reply,
            owner: "ExecutionSupervisor",
            producers: &["ExecutionSupervisor"],
            consumer: "ExecutionTerminalWaiter",
            scope: CommunicationScope::Request,
            delivery: DeliveryGuarantee::InMemory,
            capacity: CapacityPolicy::One,
            overflow: OverflowPolicy::NotApplicable,
            closure: ClosureMeaning::RuntimeUnavailable,
            cancellation: CancellationMeaning::ScopeShutdown,
        }
    );
    contract!(
        ExecutionHandoffAck,
        ReplyContract,
        EXECUTION_HANDOFF_ACK,
        CommunicationSpec {
            id: "orchd.execution.handoff_ack",
            kind: CommunicationKind::Reply,
            owner: "ExecutionTerminalWaiter",
            producers: &["AgentActor"],
            consumer: "ExecutionTerminalWaiter",
            scope: CommunicationScope::Request,
            delivery: DeliveryGuarantee::InMemory,
            capacity: CapacityPolicy::One,
            overflow: OverflowPolicy::NotApplicable,
            closure: ClosureMeaning::RuntimeUnavailable,
            cancellation: CancellationMeaning::DropInterestOnly,
        }
    );
    contract!(
        SessionReliableObservation,
        BroadcastContract,
        SESSION_RELIABLE_OBSERVATION,
        CommunicationSpec {
            id: "orchd.session.reliable_observation",
            kind: CommunicationKind::Observation,
            owner: "SessionOutputHub",
            producers: &["SessionObservationRouter"],
            consumer: "HostObservationProjection",
            scope: CommunicationScope::Session,
            delivery: DeliveryGuarantee::ReliableObservation,
            capacity: CapacityPolicy::Bounded(64),
            overflow: OverflowPolicy::RecoverSnapshot,
            closure: ClosureMeaning::ObservationEnded,
            cancellation: CancellationMeaning::ScopeShutdown,
        }
    );
    contract!(
        SessionRealtimeObservation,
        BroadcastContract,
        SESSION_REALTIME_OBSERVATION,
        CommunicationSpec {
            id: "orchd.session.realtime_observation",
            kind: CommunicationKind::Observation,
            owner: "SessionOutputHub",
            producers: &["SessionObservationRouter"],
            consumer: "HostObservationProjection",
            scope: CommunicationScope::Session,
            delivery: DeliveryGuarantee::BestEffort,
            capacity: CapacityPolicy::Bounded(256),
            overflow: OverflowPolicy::DropNewest,
            closure: ClosureMeaning::ObservationEnded,
            cancellation: CancellationMeaning::ScopeShutdown,
        }
    );
    contract!(
        ApprovalReply,
        ReplyContract,
        APPROVAL_REPLY,
        CommunicationSpec {
            id: "hostd.prompt.approval_reply",
            kind: CommunicationKind::Reply,
            owner: "OrchAgentRunRunner",
            producers: &["HostApp"],
            consumer: "ApprovalGateway",
            scope: CommunicationScope::Request,
            delivery: DeliveryGuarantee::InMemory,
            capacity: CapacityPolicy::One,
            overflow: OverflowPolicy::NotApplicable,
            closure: ClosureMeaning::RequestCancelled,
            cancellation: CancellationMeaning::DropInterestOnly,
        }
    );
    contract!(
        InteractionReply,
        ReplyContract,
        INTERACTION_REPLY,
        CommunicationSpec {
            id: "hostd.prompt.interaction_reply",
            kind: CommunicationKind::Reply,
            owner: "OrchAgentRunRunner",
            producers: &["HostApp"],
            consumer: "UserInteractionGateway",
            scope: CommunicationScope::Request,
            delivery: DeliveryGuarantee::InMemory,
            capacity: CapacityPolicy::One,
            overflow: OverflowPolicy::NotApplicable,
            closure: ClosureMeaning::RequestCancelled,
            cancellation: CancellationMeaning::DropInterestOnly,
        }
    );
    contract!(
        TuiHostBridge,
        ThreadBridgeContract,
        TUI_HOST_BRIDGE,
        CommunicationSpec {
            id: "tui.host.process_bridge",
            kind: CommunicationKind::ThreadBridge,
            owner: "HostdClient",
            producers: &["HostStdoutReaderThread"],
            consumer: "TuiEventLoop",
            scope: CommunicationScope::Process,
            delivery: DeliveryGuarantee::InMemory,
            capacity: CapacityPolicy::Unbounded {
                justification: "blocking host stdout reader crosses into the synchronous TUI loop",
            },
            overflow: OverflowPolicy::NotApplicable,
            closure: ClosureMeaning::ProcessExited,
            cancellation: CancellationMeaning::DisconnectOnly,
        }
    );
    contract!(
        GuiHostBridge,
        ThreadBridgeContract,
        GUI_HOST_BRIDGE,
        CommunicationSpec {
            id: "gui.host.process_bridge",
            kind: CommunicationKind::ThreadBridge,
            owner: "GuiHostTransport",
            producers: &["GuiHostStdoutReaderThread"],
            consumer: "GuiClientBridgePoll",
            scope: CommunicationScope::Process,
            delivery: DeliveryGuarantee::InMemory,
            capacity: CapacityPolicy::Unbounded {
                justification: "blocking host stdout reader crosses into the GPUI foreground poll loop",
            },
            overflow: OverflowPolicy::NotApplicable,
            closure: ClosureMeaning::ProcessExited,
            cancellation: CancellationMeaning::DisconnectOnly,
        }
    );
    contract!(
        HostCommandOutput,
        MailboxContract,
        HOST_COMMAND_OUTPUT,
        CommunicationSpec {
            id: "hostd.client.output",
            kind: CommunicationKind::ClientOutput,
            owner: "HostClientConnection",
            producers: &["HostApp", "HostServerCommandTask"],
            consumer: "HostTransport",
            scope: CommunicationScope::Connection,
            delivery: DeliveryGuarantee::InMemory,
            capacity: CapacityPolicy::Bounded(256),
            overflow: OverflowPolicy::AwaitCapacity,
            closure: ClosureMeaning::ClientDisconnected,
            cancellation: CancellationMeaning::DisconnectOnly,
        }
    );

    pub const ALL: &[CommunicationSpec] = &[
        AGENT_COMMANDS,
        AGENT_COMMAND_REPLY,
        AGENT_RUN_STARTED,
        AGENT_RUN_REPORT,
        AGENT_SNAPSHOT,
        EXECUTION_COMMANDS,
        EXECUTION_COMMAND_REPLY,
        EXECUTION_TERMINAL,
        EXECUTION_HANDOFF_ACK,
        SESSION_RELIABLE_OBSERVATION,
        SESSION_REALTIME_OBSERVATION,
        APPROVAL_REPLY,
        INTERACTION_REPLY,
        TUI_HOST_BRIDGE,
        GUI_HOST_BRIDGE,
        HOST_COMMAND_OUTPUT,
    ];
}

pub const ALL_SPECS: &[CommunicationSpec] = contracts::ALL;

#[cfg(test)]
mod tests {
    #[test]
    fn catalog_is_valid() {
        if let Err(errors) = crate::validate_catalog(super::ALL_SPECS) {
            panic!("invalid communication catalog: {errors:#?}");
        }
    }
}
