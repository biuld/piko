// ---- Generic actor kernel: ActorSystem ----

import type { ActorId, Envelope } from "./envelope.js";
import { createEnvelope } from "./envelope.js";
import { ActorNotFoundError, ActorStoppedError, AskTimeoutError } from "./errors.js";
import { type DeferredAsk, Mailbox } from "./mailbox.js";

// ---- Types ----

export interface ActorRef {
  id: ActorId;
  kind?: string;
}

export interface SpawnSpec {
  id: ActorId;
  kind?: string;
  handler: ActorHandler;
}

export interface SendOptions {
  correlationId?: string;
  deadlineMs?: number;
}

export interface AskOptions {
  deadlineMs?: number;
}

export interface ActorContext {
  readonly self: ActorRef;

  send(target: ActorId, msg: unknown, options?: SendOptions): void;
  ask<T>(target: ActorId, msg: unknown, options?: AskOptions): Promise<T>;
  reply<T>(meta: Envelope, result: T): void;
  reject(meta: Envelope, error: unknown): void;

  spawn(spec: SpawnSpec): ActorRef;
  stop(target: ActorId, reason?: string): Promise<void>;

  now(): number;
}

export type ActorHandler<Msg = unknown> = (
  msg: Msg,
  ctx: ActorContext,
  meta: Envelope<Msg>,
) => Promise<void> | void;

interface ActorCell {
  ref: ActorRef;
  mailbox: Mailbox;
  handler: ActorHandler;
}

// ---- ActorSystem ----

export class ActorSystem {
  private actors = new Map<ActorId, ActorCell>();
  private pendingAsks = new Map<string, DeferredAsk>();

  constructor(opts?: { defaultMailboxCapacity?: number }) {
    this.defaultMailboxCapacity = opts?.defaultMailboxCapacity ?? 100;
  }

  private defaultMailboxCapacity: number;

  // ---- Spawn / Stop ----

  spawn(spec: SpawnSpec): ActorRef {
    if (this.actors.has(spec.id)) {
      return this.actors.get(spec.id)!.ref;
    }

    const mailbox = new Mailbox(spec.id, {
      capacity: this.defaultMailboxCapacity,
    });

    const ref: ActorRef = { id: spec.id, kind: spec.kind };

    const ctx = this.createContext(ref);

    mailbox.setHandler(async (env: Envelope) => {
      try {
        await (
          spec.handler as (msg: unknown, ctx: ActorContext, meta: Envelope) => Promise<void> | void
        )(env.payload, ctx, env);
      } catch (err) {
        // If this was an ask, reject it
        if (env.correlationId) {
          this.rejectAsk(env.correlationId, err);
        }
        // Emit error to system-level handler if configured
        console.error(`[ActorSystem] actor "${spec.id}" error:`, err);
      }
    });

    this.actors.set(spec.id, { ref, mailbox, handler: spec.handler as ActorHandler });
    return ref;
  }

  async stop(actorId: string, _reason?: string): Promise<void> {
    const cell = this.actors.get(actorId);
    if (!cell) return;

    // Close mailbox (prevents new messages)
    cell.mailbox.stop();

    // Reject all pending asks targeting this actor
    for (const [correlationId, pending] of this.pendingAsks) {
      if (pending.envelope.to === actorId) {
        const err = new ActorStoppedError(actorId);
        pending.deferred.reject(err);
        this.pendingAsks.delete(correlationId);
      }
    }

    this.actors.delete(actorId);
  }

  async stopAll(reason?: string): Promise<void> {
    const ids = [...this.actors.keys()];
    await Promise.all(ids.map((id) => this.stop(id, reason)));
  }

  // ---- Messaging ----

  send(to: ActorId, msg: unknown, from?: ActorId, options?: SendOptions): void {
    const cell = this.actors.get(to);
    if (!cell) throw new ActorNotFoundError(to);

    const envelope = createEnvelope(to, msg, from, {
      correlationId: options?.correlationId,
      deadlineMs: options?.deadlineMs,
    });
    cell.mailbox.enqueue(envelope);
  }

  ask<T>(to: ActorId, msg: unknown, from?: ActorId, options?: AskOptions): Promise<T> {
    const cell = this.actors.get(to);
    if (!cell) throw new ActorNotFoundError(to);

    const correlationId = `ask_${Date.now()}_${Math.random().toString(36).slice(2)}`;

    const envelope = createEnvelope(to, msg, from, {
      correlationId,
      replyTo: from,
      deadlineMs: options?.deadlineMs,
    });

    return new Promise<T>((resolve, reject) => {
      this.pendingAsks.set(correlationId, {
        envelope,
        deferred: { resolve: resolve as (value: unknown) => void, reject },
      });

      if (options?.deadlineMs) {
        setTimeout(() => {
          if (this.pendingAsks.has(correlationId)) {
            this.pendingAsks.delete(correlationId);
            reject(new AskTimeoutError(to, correlationId));
          }
        }, options.deadlineMs);
      }

      try {
        cell.mailbox.enqueue(envelope);
      } catch (err) {
        this.pendingAsks.delete(correlationId);
        reject(err);
      }
    });
  }

  reply(meta: Envelope, result: unknown): void {
    // Only resolve the pending ask promise; do NOT enqueue back to the caller.
    // Callers that need a message should use send() separately.
    if (meta.correlationId) {
      this.resolveAsk(meta.correlationId, result);
    }
  }

  reject(meta: Envelope, error: unknown): void {
    if (meta.correlationId) {
      this.rejectAsk(meta.correlationId, error);
    }
  }

  // ---- Internal ----

  private resolveAsk(correlationId: string, result: unknown): void {
    const pending = this.pendingAsks.get(correlationId);
    if (pending) {
      this.pendingAsks.delete(correlationId);
      pending.deferred.resolve(result);
    }
  }

  private rejectAsk(correlationId: string, error: unknown): void {
    const pending = this.pendingAsks.get(correlationId);
    if (pending) {
      this.pendingAsks.delete(correlationId);
      pending.deferred.reject(error);
    }
  }

  private createContext(ref: ActorRef): ActorContext {
    return {
      self: ref,

      send: (target, msg, opts) => {
        this.send(target, msg, ref.id, opts);
      },

      ask: <T>(target: ActorId, msg: unknown, opts?: AskOptions) => {
        return this.ask<T>(target, msg, ref.id, opts);
      },

      reply: <T>(meta: Envelope, result: T) => {
        this.reply(meta, result);
      },

      reject: (meta: Envelope, error: unknown) => {
        this.reject(meta, error);
      },

      spawn: (spec: SpawnSpec) => {
        return this.spawn(spec);
      },

      stop: (target, reason) => {
        return this.stop(target, reason);
      },

      now: () => Date.now(),
    };
  }

  hasActor(id: ActorId): boolean {
    return this.actors.has(id);
  }

  getActorIds(): ActorId[] {
    return [...this.actors.keys()];
  }
}
