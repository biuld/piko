import type { Editor } from "@earendil-works/pi-tui";
import type { TuiContext } from "./context.js";

export function isImageData(buf: Buffer): boolean {
  if (buf.length < 4) return false;
  if (buf[0] === 0x89 && buf[1] === 0x50 && buf[2] === 0x4e && buf[3] === 0x47) return true; // PNG
  if (buf[0] === 0xff && buf[1] === 0xd8) return true; // JPEG
  if (buf[0] === 0x47 && buf[1] === 0x49 && buf[2] === 0x46 && buf[3] === 0x38) return true; // GIF
  if (buf[0] === 0x52 && buf[1] === 0x49 && buf[2] === 0x46 && buf[3] === 0x46 &&
      buf.length > 12 && buf[8] === 0x57 && buf[9] === 0x45 && buf[10] === 0x42 && buf[11] === 0x50) return true; // WebP
  return false;
}

export async function handleImagePaste(
  ctx: TuiContext,
  editor: Editor,
  getEditorText: () => string,
  buf: Buffer,
): Promise<void> {
  try {
    const { writeFileSync, mkdirSync, existsSync } = await import("node:fs");
    const { join } = await import("node:path");
    const { tmpdir } = await import("node:os");
    const pikoTmp = join(tmpdir(), "piko-images");
    if (!existsSync(pikoTmp)) mkdirSync(pikoTmp, { recursive: true });
    let ext = ".png";
    if (buf[0] === 0xff && buf[1] === 0xd8) ext = ".jpg";
    else if (buf[0] === 0x47) ext = ".gif";
    else if (buf[0] === 0x52 && buf[1] === 0x49) ext = ".webp";
    const filename = `paste-${Date.now()}${ext}`;
    const filepath = join(pikoTmp, filename);
    writeFileSync(filepath, buf);
    const currentText = getEditorText();
    editor.setText(`${currentText}@${filepath} `);
    ctx.chatView.addMessage("system", `📷 Image pasted: ${filename}`);
    ctx.chatView.rebuildChat();
    ctx.tui.requestRender();
  } catch {
    ctx.chatView.addMessage("system", "Failed to process pasted image");
  }
}
