import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { build } from "esbuild";
import { inject } from "postject";

const directory = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const dist = path.join(directory, "dist");
const bundle = path.join(dist, "index.cjs");
const blob = path.join(dist, "pga-agent-bridge.blob");
const executable = path.join(dist, "pga-agent-bridge-x86_64-pc-windows-msvc.exe");
const config = path.join(dist, "sea-config.json");

await mkdir(dist, { recursive: true });
await build({
  entryPoints: [path.join(directory, "src", "index.ts")],
  bundle: true,
  platform: "node",
  format: "cjs",
  target: "node22",
  outfile: bundle,
  legalComments: "none",
});

if (process.argv.includes("--javascript-only")) process.exit(0);

await writeFile(
  config,
  JSON.stringify(
    {
      main: bundle,
      output: blob,
      disableExperimentalSEAWarning: true,
      useSnapshot: false,
      useCodeCache: false,
    },
    null,
    2,
  ),
);
const generated = spawnSync(process.execPath, [`--experimental-sea-config=${config}`], {
  cwd: directory,
  stdio: "inherit",
});
if (generated.status !== 0) throw new Error("Node SEA blob generation failed");

await copyFile(process.execPath, executable);
await inject(executable, "NODE_SEA_BLOB", await readFile(blob), {
  sentinelFuse: "NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2",
});
process.stdout.write(`${executable}\n`);
