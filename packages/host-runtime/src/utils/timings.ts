/**
 * Timing / telemetry utility — lightweight performance measurement
 * for startup benchmarks and debugging.
 */

// ============================================================================
// Types
// ============================================================================

export interface TimingEntry {
  label: string;
  start: number;
  end?: number;
  elapsed?: number;
}

// ============================================================================
// Timer
// ============================================================================

export class Timings {
  private entries = new Map<string, TimingEntry>();
  private stack: string[] = [];
  private enabled: boolean;

  constructor(enabled: boolean = false) {
    this.enabled = enabled || process.env.PIKO_STARTUP_BENCHMARK === "1";
  }

  /** Start timing a section. */
  time(label: string): void {
    if (!this.enabled) return;
    const entry: TimingEntry = { label, start: performance.now() };
    this.entries.set(label, entry);
    this.stack.push(label);
  }

  /** End the most recent section. */
  timeEnd(): void {
    if (!this.enabled) return;
    const label = this.stack.pop();
    if (!label) return;
    const entry = this.entries.get(label);
    if (entry) {
      entry.end = performance.now();
      entry.elapsed = entry.end - entry.start;
    }
  }

  /** End a specific section. */
  timeEndLabel(label: string): void {
    if (!this.enabled) return;
    const entry = this.entries.get(label);
    if (entry) {
      entry.end = performance.now();
      entry.elapsed = entry.end - entry.start;
    }
  }

  /** Get all timing results. */
  getResults(): Array<{ label: string; elapsedMs: number }> {
    const results: Array<{ label: string; elapsedMs: number }> = [];
    for (const [, entry] of this.entries) {
      if (entry.elapsed !== undefined) {
        results.push({ label: entry.label, elapsedMs: Math.round(entry.elapsed * 100) / 100 });
      }
    }
    return results;
  }

  /** Print formatted timing report to stderr. */
  printTimings(): void {
    if (!this.enabled) return;
    const results = this.getResults();
    if (results.length === 0) return;

    const maxLabelLen = Math.max(...results.map((r) => r.label.length));

    process.stderr.write("\n── Startup Timing ──\n");
    for (const { label, elapsedMs } of results) {
      const padded = label.padEnd(maxLabelLen);
      process.stderr.write(`  ${padded}  ${elapsedMs.toFixed(1)}ms\n`);
    }

    const total = results.reduce((sum, r) => sum + r.elapsedMs, 0);
    process.stderr.write(`  ${"TOTAL".padEnd(maxLabelLen)}  ${total.toFixed(1)}ms\n\n`);
  }

  /** Check if timing is enabled. */
  isEnabled(): boolean {
    return this.enabled;
  }
}

// ============================================================================
// Singleton
// ============================================================================

let globalTimings: Timings | null = null;

export function getTimings(): Timings {
  if (!globalTimings) {
    globalTimings = new Timings();
  }
  return globalTimings;
}

export function resetTimings(): void {
  globalTimings = new Timings();
}
