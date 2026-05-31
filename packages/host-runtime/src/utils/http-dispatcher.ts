/**
 * HTTP dispatcher — configures global HTTP connection settings.
 *
 * Configures keep-alive and idle timeout on the global HTTP/HTTPS agents.
 */

import http from "node:http";
import https from "node:https";

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

  const timeout = Math.max(0, Math.floor(timeoutMs));

  // Use any cast to avoid TS strict issues with Agent types
  const httpAgent = http.globalAgent as any;
  const httpsAgent = https.globalAgent as any;

  if (timeout > 0) {
    httpAgent.keepAlive = true;
    httpAgent.keepAliveMsecs = timeout;
    httpAgent.timeout = timeout;

    httpsAgent.keepAlive = true;
    httpsAgent.keepAliveMsecs = timeout;
    httpsAgent.timeout = timeout;
  }
}

/**
 * Apply HTTP dispatcher settings from user config.
 */
export function applyHttpSettings(options: { httpIdleTimeoutMs?: number }): void {
  const timeoutMs = options.httpIdleTimeoutMs ?? DEFAULT_HTTP_IDLE_TIMEOUT_MS;
  configureHttpDispatcher(timeoutMs);
}
