// ---- Generic actor kernel: Envelope ----

export type ActorId = string;

let _nextId = 0;
function nextId(): string {
  _nextId++;
  return `env_${_nextId}`;
}

export interface Envelope<T = unknown> {
  id: string;
  to: ActorId;
  from?: ActorId;
  payload: T;
  correlationId?: string;
  replyTo?: ActorId;
  causationId?: string;
  createdAt: number;
  deadlineAt?: number;
}

export function createEnvelope<T>(
  to: ActorId,
  payload: T,
  from?: ActorId,
  opts?: { correlationId?: string; replyTo?: ActorId; deadlineMs?: number },
): Envelope<T> {
  return {
    id: nextId(),
    to,
    from,
    payload,
    correlationId: opts?.correlationId,
    replyTo: opts?.replyTo ?? from,
    createdAt: Date.now(),
    deadlineAt: opts?.deadlineMs ? Date.now() + opts.deadlineMs : undefined,
  };
}
