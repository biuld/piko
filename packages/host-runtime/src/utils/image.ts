/**
 * Image utilities — basic image handling for piko.
 *
 * Features:
 * - Image size parsing from Buffer (PNG, JPEG, GIF, WebP headers)
 * - Image dimension estimation for token counting
 * - Auto-resize flag (resize logic deferred to native integration)
 *
 * For full resize/clipboard support, integrate with sharp or native APIs.
 */

// ============================================================================
// Types
// ============================================================================

export interface ImageDimensions {
  width: number;
  height: number;
  format: string;
}

export interface ImageResizeOptions {
  /** Max width. Default: 2000 */
  maxWidth?: number;
  /** Max height. Default: 2000 */
  maxHeight?: number;
  /** Whether auto-resize is enabled. Default: true */
  autoResize?: boolean;
}

// ============================================================================
// Image header parsing (no native deps)
// ============================================================================

function parsePng(buffer: Buffer): ImageDimensions | null {
  if (buffer.length < 24) return null;
  if (buffer.readUInt32BE(0) !== 0x89504e47) return null; // PNG magic
  return {
    width: buffer.readUInt32BE(16),
    height: buffer.readUInt32BE(20),
    format: "png",
  };
}

function parseJpeg(buffer: Buffer): ImageDimensions | null {
  if (buffer.length < 3) return null;
  if (buffer[0] !== 0xff || buffer[1] !== 0xd8) return null;

  let offset = 2;
  while (offset < buffer.length - 9) {
    if (buffer[offset] !== 0xff) return null;
    const marker = buffer[offset + 1];

    // SOFn markers (Start of Frame)
    if (marker >= 0xc0 && marker <= 0xcf && marker !== 0xc4 && marker !== 0xc8 && marker !== 0xcc) {
      return {
        width: buffer.readUInt16BE(offset + 7),
        height: buffer.readUInt16BE(offset + 5),
        format: "jpeg",
      };
    }

    offset += 2 + buffer.readUInt16BE(offset + 2);
  }

  return null;
}

function parseGif(buffer: Buffer): ImageDimensions | null {
  if (buffer.length < 10) return null;
  if (buffer.toString("ascii", 0, 3) !== "GIF") return null;
  return {
    width: buffer.readUInt16LE(6),
    height: buffer.readUInt16LE(8),
    format: "gif",
  };
}

function parseWebP(buffer: Buffer): ImageDimensions | null {
  if (buffer.length < 30) return null;

  // RIFF header
  if (buffer.toString("ascii", 0, 4) !== "RIFF") return null;
  if (buffer.toString("ascii", 8, 12) !== "WEBP") return null;

  // VP8 / VP8L / VP8X
  const chunkType = buffer.toString("ascii", 12, 16);

  if (chunkType === "VP8 " && buffer.length >= 30) {
    return {
      width: buffer.readUInt16LE(26) & 0x3fff,
      height: buffer.readUInt16LE(28) & 0x3fff,
      format: "webp",
    };
  }

  if (chunkType === "VP8L" && buffer.length >= 25) {
    const bits = buffer.readUInt32LE(21);
    return {
      width: (bits & 0x3fff) + 1,
      height: ((bits >> 14) & 0x3fff) + 1,
      format: "webp",
    };
  }

  if (chunkType === "VP8X" && buffer.length >= 30) {
    return {
      width: (buffer.readUInt32LE(24) & 0x00ffffff) + 1,
      height:
        (((buffer.readUInt32LE(26) & 0xffff0000) >> 16) |
          ((buffer.readUInt32LE(28) & 0x0000ffff) << 16)) +
        1,
      format: "webp",
    };
  }

  return null;
}

// ============================================================================
// Public API
// ============================================================================

/**
 * Get image dimensions from a Buffer. Supports PNG, JPEG, GIF, WebP.
 * Returns null if format is unsupported or buffer is invalid.
 */
export function getImageDimensions(buffer: Buffer): ImageDimensions | null {
  return parsePng(buffer) ?? parseJpeg(buffer) ?? parseGif(buffer) ?? parseWebP(buffer);
}

/**
 * Check if a buffer is an image (by magic bytes).
 */
export function isImage(buffer: Buffer): boolean {
  return getImageDimensions(buffer) !== null;
}

/**
 * Detect image format from a file path extension.
 */
export function getImageFormatFromPath(filePath: string): string | null {
  const ext = filePath.split(".").pop()?.toLowerCase();
  const formats: Record<string, string> = {
    png: "png",
    jpg: "jpeg",
    jpeg: "jpeg",
    gif: "gif",
    webp: "webp",
    bmp: "bmp",
  };
  return formats[ext ?? ""] ?? null;
}

/**
 * Check whether an image should be auto-resized.
 * Returns true if dimensions exceed the max and auto-resize is enabled.
 */
export function shouldResize(dims: ImageDimensions, options: ImageResizeOptions = {}): boolean {
  if (options.autoResize === false) return false;
  const maxW = options.maxWidth ?? 2000;
  const maxH = options.maxHeight ?? 2000;
  return dims.width > maxW || dims.height > maxH;
}

/**
 * Estimate token count for an image.
 * Approximate: width * height * 0.25 tokens (clamped).
 */
export function estimateImageTokens(dims: ImageDimensions): number {
  const pixels = dims.width * dims.height;
  return Math.ceil(Math.min(pixels * 0.25, 2048));
}

// ============================================================================
// Image content helper (for Message format)
// ============================================================================

export interface ImageAttachment {
  data: string; // base64-encoded data
  mimeType: string;
  width: number;
  height: number;
}

/**
 * Create an image attachment from a Buffer.
 */
export function createImageAttachment(buffer: Buffer): ImageAttachment | null {
  const dims = getImageDimensions(buffer);
  if (!dims) return null;

  const mimeMap: Record<string, string> = {
    png: "image/png",
    jpeg: "image/jpeg",
    gif: "image/gif",
    webp: "image/webp",
  };

  return {
    data: buffer.toString("base64"),
    mimeType: mimeMap[dims.format] ?? "image/png",
    width: dims.width,
    height: dims.height,
  };
}
