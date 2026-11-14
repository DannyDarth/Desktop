import { build } from "../../node_modules/esbuild/lib/main.js";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
const here = path.dirname(fileURLToPath(import.meta.url));
const temp = fs.mkdtempSync(path.join(os.tmpdir(), "openmouse-routing-"));
try {
  const output = path.join(temp, "test.mjs");
  await build({ entryPoints: [path.join(here, "native-hid/keyboard-routing.test.ts")], bundle: true, platform: "node", format: "esm", outfile: output,
    alias: { "@tauri-apps/api/core": path.join(here,"shims/tauri-core.ts"), "@tauri-apps/api/event": path.join(here,"shims/tauri-event.ts") } });
  await import(pathToFileURL(output).href);
} finally { fs.rmSync(temp, { recursive:true, force:true }); }
