import { cpSync, chmodSync, mkdirSync, statSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tauriDir = join(root, "src-tauri");
const sidecarDir = join(root, "sidecar", "anemll-serverd");
const release = process.argv.includes("--release");

function run(command, args, cwd) {
  const result = spawnSync(command, args, { cwd, encoding: "utf8", stdio: "inherit" });
  if (result.error) {
    process.stderr.write(`Could not run ${command}: ${result.error.message}\n`);
    process.exit(1);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function capture(command, args, cwd) {
  const result = spawnSync(command, args, { cwd, encoding: "utf8" });
  if (result.error || result.status !== 0) {
    process.stderr.write(`Could not run ${command}: ${result.error?.message ?? result.stderr}\n`);
    process.exit(1);
  }
  return result.stdout ?? "";
}

function hostTriple() {
  const output = capture("rustc", ["-vV"], root);
  const match = output.match(/^host:\s+(.+)$/m);
  if (!match) {
    process.stderr.write("rustc did not report a host target triple\n");
    process.exit(1);
  }
  return match[1].trim();
}

function requireArtifact(path, label) {
  try {
    const artifact = statSync(path);
    if (artifact.isFile() && artifact.size > 0) return;
  } catch {}
  process.stderr.write(`${label} is missing or empty: ${path}\n`);
  process.exit(1);
}

// SwiftPM builds for the host toolchain only; there is no cross-compile
// story here (mirrors terax-cli's use of the rustc host triple, since this
// repo currently only ships macOS aarch64).
const target = hostTriple();

const swiftArgs = ["build"];
if (release) swiftArgs.push("-c", "release");
run("swift", swiftArgs, sidecarDir);

const profile = release ? "release" : "debug";
const source = join(sidecarDir, ".build", profile, "anemll-serverd");
const destination = join(tauriDir, "binaries", `anemll-serverd-${target}`);
requireArtifact(source, "Built anemll-serverd artifact");
mkdirSync(dirname(destination), { recursive: true });
cpSync(source, destination);
chmodSync(destination, 0o755);
requireArtifact(destination, "Prepared anemll-serverd sidecar");

console.log(`Prepared ${destination.slice(root.length + 1)}`);
