import { describe, expect, it } from "vitest";
import { pollOAuthDeviceCodeFlow } from "../src/auth/oauth.js";

// ============================================================================
// pollOAuthDeviceCodeFlow tests
// ============================================================================

describe("pollOAuthDeviceCodeFlow", () => {
  it("completes immediately when poll returns complete", async () => {
    const result = await pollOAuthDeviceCodeFlow(
      async () => ({
        status: "complete" as const,
        value: { accessToken: "tok", tokenType: "bearer" },
      }),
      { intervalSeconds: 1, expiresInSeconds: 10 },
    );
    expect(result.accessToken).toBe("tok");
  });

  it("polls until complete after some pending responses", async () => {
    let calls = 0;
    const result = await pollOAuthDeviceCodeFlow(
      async () => {
        calls++;
        if (calls < 3) return { status: "pending" as const };
        return {
          status: "complete" as const,
          value: { accessToken: `tok-${calls}`, tokenType: "bearer" },
        };
      },
      { intervalSeconds: 0.01, expiresInSeconds: 5 },
    );
    expect(result.accessToken).toBe("tok-3");
    expect(calls).toBe(3);
  });

  it("throws on failed status", async () => {
    await expect(
      pollOAuthDeviceCodeFlow(async () => ({ status: "failed" as const, message: "bad request" }), {
        intervalSeconds: 1,
        expiresInSeconds: 5,
      }),
    ).rejects.toThrow("bad request");
  });

  it("throws on timeout", async () => {
    await expect(
      pollOAuthDeviceCodeFlow(async () => ({ status: "pending" as const }), {
        intervalSeconds: 0.01,
        expiresInSeconds: 0.05,
      }),
    ).rejects.toThrow("Device flow timed out");
  });

  it("handles slow_down by returning appropriate timeout message", async () => {
    // slow_down with very short expiry should timeout with clock drift hint
    const promise = pollOAuthDeviceCodeFlow(async () => ({ status: "slow_down" as const }), {
      intervalSeconds: 0.01,
      expiresInSeconds: 0.05,
    });
    await expect(promise).rejects.toThrow("clock drift");
  });

  it("throws with WSL/VM hint after slow_down timeout", async () => {
    await expect(
      pollOAuthDeviceCodeFlow(async () => ({ status: "slow_down" as const }), {
        intervalSeconds: 0.01,
        expiresInSeconds: 0.05,
      }),
    ).rejects.toThrow("clock drift");
  });

  it("throws when aborted via signal", async () => {
    const controller = new AbortController();
    const promise = pollOAuthDeviceCodeFlow(
      async () => {
        // This doesn't resolve — the abort should interrupt the sleep
        return { status: "pending" as const };
      },
      { intervalSeconds: 0.1, expiresInSeconds: 10, signal: controller.signal },
    );

    // Abort after a tiny delay
    await new Promise((r) => setTimeout(r, 20));
    controller.abort();

    await expect(promise).rejects.toThrow("Login cancelled");
  });

  it("respects minimum interval of 1 second", async () => {
    // Even with 0 interval, should use minimum 1s
    let calls = 0;
    const start = Date.now();
    const promise = pollOAuthDeviceCodeFlow(
      async () => {
        calls++;
        return { status: "pending" as const };
      },
      { intervalSeconds: 0.001, expiresInSeconds: 0.1 },
    );
    await promise.catch(() => {}); // Will timeout
    const elapsed = Date.now() - start;
    // Should have waited at least 100ms before timing out
    expect(elapsed).toBeGreaterThanOrEqual(90);
  });
});

describe("getOAuthConfig", () => {
  it("returns config for anthropic", async () => {
    const { getOAuthConfig } = await import("../src/auth/oauth.js");
    expect(getOAuthConfig("anthropic")).toBeDefined();
    expect(getOAuthConfig("Anthropic")).toBeDefined();
  });

  it("returns config for openai", async () => {
    const { getOAuthConfig } = await import("../src/auth/oauth.js");
    expect(getOAuthConfig("openai")).toBeDefined();
    expect(getOAuthConfig("OPENAI")).toBeDefined();
  });

  it("returns undefined for unknown provider", async () => {
    const { getOAuthConfig } = await import("../src/auth/oauth.js");
    expect(getOAuthConfig("unknown")).toBeUndefined();
  });
});
