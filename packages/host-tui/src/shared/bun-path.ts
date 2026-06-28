function normalizeSlashes(path: string): string {
  return path.replace(/\\/g, "/");
}

function hasDrivePrefix(path: string): boolean {
  return /^[a-zA-Z]:\//.test(path);
}

export function isAbsolutePath(path: string): boolean {
  const normalized = normalizeSlashes(path);
  return normalized.startsWith("/") || hasDrivePrefix(normalized);
}

export function joinPath(...parts: string[]): string {
  const filtered = parts.filter((part) => part.length > 0);
  if (filtered.length === 0) return ".";

  const first = normalizeSlashes(filtered[0]!);
  const absolute = first.startsWith("/");
  const drive = hasDrivePrefix(first) ? first.slice(0, 2) : "";
  const segments: string[] = [];

  for (const rawPart of filtered) {
    const part = normalizeSlashes(rawPart);
    for (const segment of part.split("/")) {
      if (!segment || segment === ".") continue;
      if (segment === "..") {
        if (segments.length > 0 && segments[segments.length - 1] !== "..") {
          segments.pop();
        } else if (!absolute && !drive) {
          segments.push(segment);
        }
        continue;
      }
      if (/^[a-zA-Z]:$/.test(segment)) continue;
      segments.push(segment);
    }
  }

  const prefix = absolute ? "/" : drive ? `${drive}/` : "";
  const result = `${prefix}${segments.join("/")}`;
  return result || (absolute ? "/" : drive || ".");
}

export function resolvePath(...parts: string[]): string {
  let resolved = "";
  for (const part of parts) {
    if (!part) continue;
    const normalized = normalizeSlashes(part);
    if (isAbsolutePath(normalized)) {
      resolved = normalized;
    } else {
      resolved = resolved ? joinPath(resolved, normalized) : normalized;
    }
  }
  return joinPath(isAbsolutePath(resolved) ? resolved : joinPath(process.cwd(), resolved));
}

export function dirnamePath(path: string): string {
  const normalized = normalizeSlashes(path).replace(/\/+$/, "");
  if (!normalized) return ".";
  const index = normalized.lastIndexOf("/");
  if (index < 0) return ".";
  if (index === 0) return "/";
  return normalized.slice(0, index);
}

export function basenamePath(path: string, suffix = ""): string {
  const normalized = normalizeSlashes(path).replace(/\/+$/, "");
  if (!normalized) return "";
  const index = normalized.lastIndexOf("/");
  const base = index < 0 ? normalized : normalized.slice(index + 1);
  return suffix && base.endsWith(suffix) ? base.slice(0, -suffix.length) : base;
}

export function extnamePath(path: string): string {
  const base = basenamePath(path);
  const index = base.lastIndexOf(".");
  return index <= 0 ? "" : base.slice(index);
}
