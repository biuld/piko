import type { Editor } from "@earendil-works/pi-tui";
import { getImageDimensions, shouldResize } from "piko-host-runtime";
import type { BaseApp } from "./base.js";

/** Maximum dimensions for pasted images before resize warning. */
const MAX_IMAGE_WIDTH = 2000;
const MAX_IMAGE_HEIGHT = 2000;

export function isImageData(buf: Buffer): boolean {
  if (buf.length < 4) return false;
  if (buf[0] === 0x89 && buf[1] === 0x50 && buf[2] === 0x4e && buf[3] === 0x47) return true;
  if (buf[0] === 0xff && buf[1] === 0xd8) return true;
  if (buf[0] === 0x47 && buf[1] === 0x49 && buf[2] === 0x46 && buf[3] === 0x38) return true;
  if (
    buf[0] === 0x52 &&
    buf[1] === 0x49 &&
    buf[2] === 0x46 &&
    buf[3] === 0x46 &&
    buf.length > 12 &&
    buf[8] === 0x57 &&
    buf[9] === 0x45 &&
    buf[10] === 0x42 &&
    buf[11] === 0x50
  )
    return true;
  return false;
}

export async function handleImagePaste(
  app: BaseApp,
  editor: Editor,
  getEditorText: () => string,
  buf: Buffer,
): Promise<void> {
  try {
    const { writeFileSync, mkdirSync, existsSync } = await import("node:fs");
    const { join } = await import("node:path");
    const { tmpdir } = await import("node:os");
    const dir = join(tmpdir(), "piko-images");
    if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
    let ext = ".png";
    if (buf[0] === 0xff && buf[1] === 0xd8) ext = ".jpg";
    else if (buf[0] === 0x47) ext = ".gif";
    else if (buf[0] === 0x52 && buf[1] === 0x49) ext = ".webp";
    const fp = join(dir, `paste-${Date.now()}${ext}`);
    writeFileSync(fp, buf);
    editor.setText(`${getEditorText()}@${fp} `);

    // Check dimensions and warn if image is very large
    const dims = getImageDimensions(buf);
    if (dims && shouldResize(dims, { maxWidth: MAX_IMAGE_WIDTH, maxHeight: MAX_IMAGE_HEIGHT })) {
      const sizeKB = (buf.length / 1024).toFixed(0);
      app.chatView.addMessage(
        "system",
        `📷 Image pasted: ${fp.split("/").pop()} (${dims.width}×${dims.height}, ${sizeKB}KB)` +
          ` — large image, may use many tokens`,
      );
    } else {
      app.chatView.addMessage("system", `📷 Image pasted: ${fp.split("/").pop()}`);
    }
  } catch {
    app.chatView.addMessage("system", "Failed to process pasted image");
  }
  app.chatView.rebuildChat();
  app.tui.requestRender();
}
