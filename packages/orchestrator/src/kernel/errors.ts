// ---- Generic actor kernel: Errors ----

export class MailboxFullError extends Error {
  constructor(actorId: string) {
    super(`Mailbox full for actor "${actorId}"`);
    this.name = "MailboxFullError";
  }
}

export class ActorStoppedError extends Error {
  constructor(actorId: string) {
    super(`Actor "${actorId}" has been stopped`);
    this.name = "ActorStoppedError";
  }
}

export class AskTimeoutError extends Error {
  constructor(targetId: string, correlationId: string) {
    super(`ask() to "${targetId}" timed out (correlationId: ${correlationId})`);
    this.name = "AskTimeoutError";
  }
}

export class ActorNotFoundError extends Error {
  constructor(actorId: string) {
    super(`Actor "${actorId}" not found`);
    this.name = "ActorNotFoundError";
  }
}
