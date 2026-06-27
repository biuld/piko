/** HTTP dispatcher settings retained for configuration compatibility. */

// ============================================================================
// Constants
// ============================================================================

/** Default HTTP idle timeout in ms (5 minutes). */
export const DEFAULT_HTTP_IDLE_TIMEOUT_MS = 300_000;

// ============================================================================
// Configuration
// ============================================================================

let configured = false;

/**
 * Configure global HTTP idle timeout for both http and https agents.
 * Safe to call multiple times — only applies once.
 * Pass 0 to disable the timeout.
 */
export function configureHttpDispatcher(timeoutMs: number = DEFAULT_HTTP_IDLE_TIMEOUT_MS): void {
  if (configured) return;
  configured = true;
  Math.max(0, Math.floor(timeoutMs));
}

/**
 * Apply HTTP dispatcher settings from user config.
 */
export function applyHttpSettings(options: { httpIdleTimeoutMs?: number }): void {
  const timeoutMs = options.httpIdleTimeoutMs ?? DEFAULT_HTTP_IDLE_TIMEOUT_MS;
  configureHttpDispatcher(timeoutMs);
}
