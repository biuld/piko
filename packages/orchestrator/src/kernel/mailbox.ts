// ---- Generic actor kernel: Mailbox ----

import type { Envelope } from "./envelope.js";
import { MailboxFullError } from "./errors.js";

export interface MailboxOptions {
  capacity?: number;
  stopTimeoutMs?: number;
}

interface Deferred<T> {
  resolve: (value: T) => void;
  reject: (error: unknown) => void;
}

export interface DeferredAsk<T = unknown> {
  envelope: Envelope;
  deferred: Deferred<T>;
}

export class Mailbox<T = unknown> {
  readonly actorId: string;
  private queue: Envelope<T>[] = [];
  private capacity: number;
  private processing = false;
  private stopped = false;
  private handler: ((env: Envelope<T>) => Promise<void>) | null = null;

  constructor(actorId: string, options: MailboxOptions = {}) {
    this.actorId = actorId;
    this.capacity = options.capacity ?? 100;
  }

  setHandler(handler: (env: Envelope<T>) => Promise<void>): void {
    this.handler = handler;
  }

  isFull(): boolean {
    return this.queue.length >= this.capacity;
  }

  enqueue(env: Envelope<T>): void {
    if (this.stopped) return;
    if (this.isFull()) {
      throw new MailboxFullError(this.actorId);
    }
    this.queue.push(env);
    this.tryNext();
  }

  tryNext(): void {
    if (this.processing || this.stopped || !this.handler) return;
    const env = this.queue.shift();
    if (!env) return;

    this.processing = true;
    this.handler(env)
      .catch(() => {
        // Handler errors are caught by the kernel, not the mailbox
      })
      .finally(() => {
        this.processing = false;
        this.tryNext();
      });
  }

  stop(): void {
    this.stopped = true;
    this.queue = [];
  }

  get isStopped(): boolean {
    return this.stopped;
  }

  get length(): number {
    return this.queue.length;
  }
}
