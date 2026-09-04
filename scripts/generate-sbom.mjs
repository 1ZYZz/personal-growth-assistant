import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const components = new Map();
const restricted = /(?:^|[^A-Z])(AGPL|SSPL|BUSL|Commons Clause)(?:[^A-Z]|$)/i;

function normalizeLicense(value) {
  if (typeof value === "string") return value.trim();
  if (value && typeof value.type === "string") return value.type.trim();
  return "UNKNOWN";
}

function addComponent(type, name, version, license, purl) {
  if (!name || !version || name === "personal-growth-assistant") return;
  const key = `${type}:${name}@${version}`;
  components.set(key, {
    type,
    name,
    version,
    licenses: [{ license: { id: license } }],
    purl,
  });
}

const pnpmStore = path.join(root, "node_modules", ".pnpm");
for (const entry of readdirSync(pnpmStore, { withFileTypes: true })) {
  if (!entry.isDirectory()) continue;
  const modules = path.join(pnpmStore, entry.name, "node_modules");
  if (!existsSync(modules)) continue;
  for (const child of readdirSync(modules, { withFileTypes: true })) {
    if (!child.isDirectory()) continue;
    const candidates = child.name.startsWith("@")
      ? readdirSync(path.join(modules, child.name), { withFileTypes: true })
          .filter((item) => item.isDirectory())
          .map((item) => path.join(modules, child.name, item.name, "package.json"))
      : [path.join(modules, child.name, "package.json")];
    for (const manifest of candidates) {
      if (!existsSync(manifest)) continue;
      const data = JSON.parse(readFileSync(manifest, "utf8"));
      const license = normalizeLicense(data.license ?? data.licenses?.[0]);
      addComponent(
        "library",
        data.name,
        data.version,
        license,
        `pkg:npm/${encodeURIComponent(data.name)}@${data.version}`,
      );
    }
  }
}

const cargo = spawnSync(
  process.env.CARGO || "cargo",
  [
    "metadata",
    "--format-version",
    "1",
    "--locked",
    "--offline",
    "--filter-platform",
    "x86_64-pc-windows-msvc",
    "--manifest-path",
    path.join(root, "apps", "desktop", "src-tauri", "Cargo.toml"),
  ],
  { cwd: root, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
);
if (cargo.status !== 0) throw new Error(cargo.stderr || "cargo metadata failed");
for (const item of JSON.parse(cargo.stdout).packages) {
  addComponent(
    "library",
    item.name,
    item.version,
    normalizeLicense(item.license),
    `pkg:cargo/${encodeURIComponent(item.name)}@${item.version}`,
  );
}

const sorted = [...components.values()].sort((left, right) =>
  `${left.purl}`.localeCompare(`${right.purl}`),
);
const problems = sorted.filter(
  (item) =>
    item.licenses[0].license.id === "UNKNOWN" || restricted.test(item.licenses[0].license.id),
);
if (problems.length) {
  throw new Error(
    `License gate rejected: ${problems
      .map((item) => `${item.name}@${item.version} (${item.licenses[0].license.id})`)
      .join(", ")}`,
  );
}

const fingerprint = createHash("sha256")
  .update(JSON.stringify(sorted.map((item) => [item.purl, item.licenses[0].license.id])))
  .digest("hex");
const uuid = `${fingerprint.slice(0, 8)}-${fingerprint.slice(8, 12)}-4${fingerprint.slice(13, 16)}-a${fingerprint.slice(17, 20)}-${fingerprint.slice(20, 32)}`;
const sbom = {
  bomFormat: "CycloneDX",
  specVersion: "1.6",
  serialNumber: `urn:uuid:${uuid}`,
  version: 1,
  metadata: {
    timestamp: new Date().toISOString(),
    component: {
      type: "application",
      name: "personal-growth-assistant",
      version: "1.1.1",
      licenses: [{ license: { id: "Apache-2.0" } }],
    },
  },
  components: sorted,
};
writeFileSync(path.join(root, "sbom.cdx.json"), `${JSON.stringify(sbom, null, 2)}\n`);
process.stdout.write(`Wrote CycloneDX 1.6 SBOM with ${sorted.length} components.\n`);
