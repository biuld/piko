/**
 * Open a URL in the default system browser.
 * Bun-native (no child_process dependency).
 */

export function openBrowser(url: string): void {
  const cmd =
    process.platform === "darwin" ? "open" : process.platform === "win32" ? "start" : "xdg-open";
  try {
    Bun.spawn([cmd, url], {
      stdio: ["ignore", "ignore", "ignore"],
    });
  } catch {
    // Silently ignore — the URL is still shown to the user
  }
}
