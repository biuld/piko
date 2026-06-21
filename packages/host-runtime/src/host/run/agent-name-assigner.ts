// ---- Agent Name Assigner ----
// Maintains a configurable name pool and assigns names in round-robin order.
// Configured via settings.json → agentNames: ["alpha", "beta", ...]
// Falls back to a built-in default pool when settings has no agentNames.

const DEFAULT_NAMES = [
  "alpha",
  "beta",
  "gamma",
  "delta",
  "epsilon",
  "zeta",
  "eta",
  "theta",
  "iota",
  "kappa",
  "lambda",
  "mu",
  "nu",
  "xi",
  "omicron",
  "pi",
  "rho",
  "sigma",
  "tau",
  "upsilon",
  "phi",
  "chi",
  "psi",
  "omega",
];

export class AgentNameAssigner {
  private index = 0;
  private names: string[];

  constructor(names?: string[]) {
    this.names = names && names.length > 0 ? names : DEFAULT_NAMES;
  }

  /** Replace the name pool entirely (e.g., when settings reload). Falls back to defaults when empty. */
  setNames(names: string[]): void {
    this.names = names.length > 0 ? names : DEFAULT_NAMES;
  }

  /** Pick the next name using round-robin. */
  next(): string {
    const name = this.names[this.index % this.names.length];
    this.index++;
    return name;
  }

  /** Reset the round-robin cursor to the beginning of the list. */
  reset(): void {
    this.index = 0;
  }
}
