"use strict";
var __create = Object.create;
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __getProtoOf = Object.getPrototypeOf;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  }
  return to;
};
var __toESM = (mod, isNodeMode, target) => (target = mod != null ? __create(__getProtoOf(mod)) : {}, __copyProps(
  // If the importer is in node compatibility mode or this is not an ESM
  // file that has been converted to a CommonJS file using a Babel-
  // compatible transform (i.e. "__esModule" has not been set), then set
  // "default" to the CommonJS "module.exports" for node compatibility.
  isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: true }) : target,
  mod
));
var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

// src/extension.ts
var extension_exports = {};
__export(extension_exports, {
  activate: () => activate,
  deactivate: () => deactivate
});
module.exports = __toCommonJS(extension_exports);
var vscode7 = __toESM(require("vscode"));

// src/toolchain/platform.ts
var os = __toESM(require("os"));
var SUPPORTED_PLATFORMS = {
  "linux-x64": "linux-x64",
  "darwin-arm64": "macos-arm64",
  "win32-x64": "windows-x64"
};
function detectPlatform(osPlatform, osArch) {
  const key = `${osPlatform ?? os.platform()}-${osArch ?? os.arch()}`;
  const id = SUPPORTED_PLATFORMS[key];
  if (!id) {
    return null;
  }
  return {
    id,
    archiveExtension: id === "windows-x64" ? ".zip" : ".tar.gz",
    binaryName: id === "windows-x64" ? "infs.exe" : "infs"
  };
}

// src/toolchain/detection.ts
var fs = __toESM(require("fs"));
var path = __toESM(require("path"));
var os2 = __toESM(require("os"));

// src/config/settings.ts
var vscode = __toESM(require("vscode"));
function getSettings() {
  const config = vscode.workspace.getConfiguration("inference");
  return {
    path: config.get("path", ""),
    autoInstall: config.get("autoInstall", true),
    channel: config.get("channel", "stable"),
    checkForUpdates: config.get("checkForUpdates", true)
  };
}

// src/toolchain/detection.ts
function inferenceHome() {
  return process.env["INFERENCE_HOME"] || path.join(os2.homedir(), ".inference");
}
function isExecutable(filePath) {
  try {
    fs.accessSync(filePath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}
function findInPath(binaryName) {
  const envPath = process.env["PATH"] || "";
  const sep = process.platform === "win32" ? ";" : ":";
  const dirs = envPath.split(sep).filter(Boolean);
  for (const dir of dirs) {
    const candidate = path.join(dir, binaryName);
    if (isExecutable(candidate)) {
      return candidate;
    }
  }
  return null;
}
function detectInfs() {
  const platform2 = detectPlatform();
  const binaryName = platform2?.binaryName ?? "infs";
  const settings = getSettings();
  if (settings.path) {
    if (isExecutable(settings.path)) {
      return settings.path;
    }
    return null;
  }
  const pathResult = findInPath(binaryName);
  if (pathResult) {
    return pathResult;
  }
  const managedPath = path.join(inferenceHome(), "bin", binaryName);
  if (isExecutable(managedPath)) {
    return managedPath;
  }
  return null;
}

// src/utils/exec.ts
var cp = __toESM(require("child_process"));
var DEFAULT_TIMEOUT_MS = 3e4;
function exec(command, args, options) {
  const timeout = options?.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  return new Promise((resolve, reject) => {
    const child = cp.spawn(command, args, {
      cwd: options?.cwd,
      stdio: ["ignore", "pipe", "pipe"],
      timeout
    });
    const stdoutChunks = [];
    const stderrChunks = [];
    child.stdout.on("data", (chunk) => stdoutChunks.push(chunk));
    child.stderr.on("data", (chunk) => stderrChunks.push(chunk));
    child.on("error", (err) => reject(err));
    child.on("close", (code) => {
      resolve({
        exitCode: code ?? 1,
        stdout: Buffer.concat(stdoutChunks).toString("utf-8"),
        stderr: Buffer.concat(stderrChunks).toString("utf-8")
      });
    });
  });
}

// src/utils/semver.ts
function compareSemver(a, b) {
  const pa = a.split("-")[0].split(".").map(Number);
  const pb = b.split("-")[0].split(".").map(Number);
  for (let i = 0; i < 3; i++) {
    const diff = (pa[i] || 0) - (pb[i] || 0);
    if (diff !== 0) {
      return diff;
    }
  }
  return 0;
}

// src/commands/install.ts
var vscode2 = __toESM(require("vscode"));

// src/toolchain/installation.ts
var fs4 = __toESM(require("fs"));
var os3 = __toESM(require("os"));
var path3 = __toESM(require("path"));

// src/utils/download.ts
var https = __toESM(require("https"));
var http = __toESM(require("http"));
var fs2 = __toESM(require("fs"));
var crypto = __toESM(require("crypto"));
var DEFAULT_TIMEOUT_MS2 = 15e3;
var MAX_REDIRECTS = 5;
var SOCKET_TIMEOUT_MS = 15e3;
function followRedirects(url, remaining) {
  return new Promise((resolve, reject) => {
    const parsed = new URL(url);
    const requester = parsed.protocol === "https:" ? https : http;
    const req = requester.get(url, (res) => {
      const status = res.statusCode ?? 0;
      if (status >= 300 && status < 400 && res.headers.location) {
        if (remaining <= 0) {
          res.resume();
          reject(new Error(`Too many redirects fetching ${url}`));
          return;
        }
        const target = new URL(res.headers.location, url).href;
        const targetProtocol = new URL(target).protocol;
        if (parsed.protocol === "https:" && targetProtocol === "http:") {
          res.resume();
          reject(
            new Error(
              `Refusing HTTPS-to-HTTP redirect: ${url} -> ${target}`
            )
          );
          return;
        }
        res.resume();
        followRedirects(target, remaining - 1).then(resolve, reject);
        return;
      }
      if (status < 200 || status >= 300) {
        res.resume();
        reject(new Error(`HTTP ${status} fetching ${url}`));
        return;
      }
      resolve(res);
    });
    req.setTimeout(SOCKET_TIMEOUT_MS, () => {
      req.destroy(new Error(`Connection timed out for ${url}`));
    });
    req.on(
      "error",
      (err) => reject(new Error(`Network error fetching ${url}: ${err.message}`))
    );
  });
}
function fetchJson(url) {
  return new Promise((resolve, reject) => {
    followRedirects(url, MAX_REDIRECTS).then(
      (res) => {
        const chunks = [];
        res.on("data", (chunk) => chunks.push(chunk));
        res.on("end", () => {
          try {
            const text = Buffer.concat(chunks).toString("utf-8");
            resolve(JSON.parse(text));
          } catch (err) {
            reject(
              new Error(
                `Failed to parse JSON from ${url}: ${err instanceof Error ? err.message : err}`
              )
            );
          }
        });
        res.on(
          "error",
          (err) => reject(
            new Error(
              `Error reading response from ${url}: ${err.message}`
            )
          )
        );
      },
      (err) => reject(err)
    );
  });
}
function downloadFile(url, options) {
  const timeout = options.timeoutMs ?? DEFAULT_TIMEOUT_MS2;
  const partialPath = options.destPath + ".partial";
  return new Promise((resolve, reject) => {
    followRedirects(url, MAX_REDIRECTS).then(
      (res) => {
        const totalStr = res.headers["content-length"];
        const total = totalStr ? parseInt(totalStr, 10) : void 0;
        let received = 0;
        const ws = fs2.createWriteStream(partialPath);
        res.on("data", (chunk) => {
          received += chunk.length;
          options.onProgress?.(received, total);
        });
        res.pipe(ws);
        const cleanup = () => {
          try {
            fs2.unlinkSync(partialPath);
          } catch {
          }
        };
        ws.on("finish", () => {
          try {
            fs2.renameSync(partialPath, options.destPath);
            resolve();
          } catch (err) {
            cleanup();
            reject(
              new Error(
                `Failed to save download to ${options.destPath}: ${err instanceof Error ? err.message : err}`
              )
            );
          }
        });
        ws.on("error", (err) => {
          cleanup();
          reject(
            new Error(
              `Failed to write download: ${err.message}`
            )
          );
        });
        res.on("error", (err) => {
          ws.destroy();
          cleanup();
          reject(
            new Error(
              `Download stream error from ${url}: ${err.message}`
            )
          );
        });
        let dataTimer;
        const resetTimer = () => {
          if (dataTimer) {
            clearTimeout(dataTimer);
          }
          dataTimer = setTimeout(() => {
            res.destroy();
            ws.destroy();
            cleanup();
            reject(new Error(`Download timed out for ${url}`));
          }, timeout);
        };
        resetTimer();
        res.on("data", resetTimer);
        res.on("end", () => {
          if (dataTimer) {
            clearTimeout(dataTimer);
          }
        });
      },
      (err) => reject(err)
    );
  });
}
function sha256File(filePath) {
  return new Promise((resolve, reject) => {
    const hash = crypto.createHash("sha256");
    const stream = fs2.createReadStream(filePath);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("end", () => resolve(hash.digest("hex")));
    stream.on(
      "error",
      (err) => reject(
        new Error(
          `Failed to compute SHA-256 for ${filePath}: ${err.message}`
        )
      )
    );
  });
}

// src/utils/extract.ts
var fs3 = __toESM(require("fs"));
var path2 = __toESM(require("path"));
async function extractArchive(options) {
  fs3.mkdirSync(options.destDir, { recursive: true });
  if (options.archivePath.endsWith(".tar.gz") || options.archivePath.endsWith(".tgz")) {
    await extractTarGz(options.archivePath, options.destDir);
  } else if (options.archivePath.endsWith(".zip")) {
    await extractZip(options.archivePath, options.destDir);
  } else {
    throw new Error(
      `Unsupported archive format: ${path2.basename(options.archivePath)}`
    );
  }
  if (process.platform !== "win32") {
    setExecutablePermissions(options.destDir);
  }
}
async function extractTarGz(archivePath, destDir) {
  const result = await exec("tar", ["-xzf", archivePath, "-C", destDir]);
  if (result.exitCode !== 0) {
    throw new Error(
      `tar extraction failed (exit ${result.exitCode}): ${result.stderr}`
    );
  }
}
async function extractZip(archivePath, destDir) {
  const result = await exec("powershell", [
    "-NoProfile",
    "-Command",
    `Expand-Archive -Path '${archivePath}' -DestinationPath '${destDir}' -Force`
  ]);
  if (result.exitCode !== 0) {
    throw new Error(
      `zip extraction failed (exit ${result.exitCode}): ${result.stderr}`
    );
  }
}
function setExecutablePermissions(dir) {
  try {
    const entries = fs3.readdirSync(dir, { withFileTypes: true });
    for (const entry of entries) {
      if (entry.isFile()) {
        const filePath = path2.join(dir, entry.name);
        fs3.chmodSync(filePath, 493);
      }
    }
  } catch {
  }
}

// src/toolchain/manifest.ts
function toolFromUrl(url) {
  const filename = url.split("/").pop() ?? "";
  return filename.split("-")[0] ?? "";
}
function osFromUrl(url) {
  const filename = url.split("/").pop() ?? "";
  const parts = filename.split("-");
  return parts.length > 1 ? parts[1] : "";
}
function platformOs(platform2) {
  if (platform2.id === "linux-x64") {
    return "linux";
  }
  if (platform2.id === "macos-arm64") {
    return "macos";
  }
  if (platform2.id === "windows-x64") {
    return "windows";
  }
  return "";
}
function findLatestRelease(manifest, platform2, channel) {
  const candidates = channel === "stable" ? manifest.filter((e) => e.stable) : manifest;
  if (candidates.length === 0) {
    return null;
  }
  const sorted = [...candidates].sort(
    (a, b) => compareSemver(b.version, a.version)
  );
  const os4 = platformOs(platform2);
  for (const release of sorted) {
    const file = release.files.find(
      (f) => toolFromUrl(f.url) === "infs" && osFromUrl(f.url) === os4
    );
    if (file) {
      return { release, fileUrl: file.url, sha256: file.sha256 };
    }
  }
  return null;
}

// src/toolchain/installation.ts
var MANIFEST_URL = "https://inference-lang.org/releases.json";
async function installToolchain(platform2, onProgress) {
  const settings = getSettings();
  const channel = settings.channel === "stable" || settings.channel === "latest" ? settings.channel : "stable";
  onProgress?.({
    stage: "fetching-manifest",
    message: "Fetching release manifest..."
  });
  const manifest = await fetchJson(MANIFEST_URL);
  const match = findLatestRelease(manifest, platform2, channel);
  if (!match) {
    throw new Error(
      `No compatible infs release found for ${platform2.id} in the ${channel} channel.`
    );
  }
  const { release, fileUrl, sha256 } = match;
  const version = release.version;
  onProgress?.({
    stage: "downloading",
    message: `Downloading infs v${version}...`
  });
  const destDir = path3.join(inferenceHome(), "bin");
  fs4.mkdirSync(destDir, { recursive: true });
  const archiveName = `infs-${platform2.id}${platform2.archiveExtension}`;
  const archivePath = path3.join(os3.tmpdir(), archiveName);
  try {
    await downloadFile(fileUrl, {
      destPath: archivePath,
      onProgress: (received, total) => {
        onProgress?.({
          stage: "downloading",
          message: `Downloading infs v${version}...`,
          bytesReceived: received,
          bytesTotal: total
        });
      }
    });
    const actualHash = await sha256File(archivePath);
    if (actualHash !== sha256) {
      throw new Error(
        `SHA-256 verification failed for infs v${version}. Expected ${sha256}, got ${actualHash}.`
      );
    }
    onProgress?.({
      stage: "extracting",
      message: "Extracting archive..."
    });
    await extractArchive({ archivePath, destDir });
  } finally {
    try {
      fs4.unlinkSync(archivePath);
    } catch {
    }
  }
  const infsPath = path3.join(destDir, platform2.binaryName);
  if (!fs4.existsSync(infsPath)) {
    throw new Error(
      `infs binary not found at ${infsPath} after extraction.`
    );
  }
  onProgress?.({
    stage: "installing",
    message: "Running infs install..."
  });
  const installResult = await exec(infsPath, ["install"], {
    timeoutMs: 12e4
  });
  if (installResult.exitCode !== 0) {
    throw new Error(
      `infs install failed (exit ${installResult.exitCode}): ${installResult.stderr || installResult.stdout}`
    );
  }
  onProgress?.({
    stage: "verifying",
    message: "Verifying installation..."
  });
  let doctorWarnings = false;
  try {
    const doctorResult = await exec(infsPath, ["doctor"], {
      timeoutMs: 3e4
    });
    if (doctorResult.exitCode !== 0) {
      doctorWarnings = true;
    }
  } catch {
    doctorWarnings = true;
  }
  return { infsPath, version, doctorWarnings };
}

// src/commands/install.ts
var installing = false;
function registerInstallCommand(outputChannel2) {
  return vscode2.commands.registerCommand(
    "inference.installToolchain",
    async () => {
      if (installing) {
        vscode2.window.showInformationMessage(
          "Inference toolchain installation is already in progress."
        );
        return;
      }
      const platform2 = detectPlatform();
      if (!platform2) {
        vscode2.window.showErrorMessage(
          `Inference: unsupported platform (${process.platform}-${process.arch}).`
        );
        return;
      }
      installing = true;
      try {
        const result = await installWithProgress(
          platform2,
          outputChannel2
        );
        outputChannel2.appendLine(
          `Toolchain v${result.version} installed at ${result.infsPath}`
        );
        notifyInstallSuccess(result.version, result.doctorWarnings);
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        outputChannel2.appendLine(`Installation failed: ${message}`);
        notifyInstallError(message);
      } finally {
        installing = false;
      }
    }
  );
}
function installWithProgress(platform2, outputChannel2) {
  return vscode2.window.withProgress(
    {
      location: vscode2.ProgressLocation.Notification,
      title: "Inference Toolchain",
      cancellable: false
    },
    async (progress) => {
      const onProgress = (p) => {
        outputChannel2.appendLine(p.message);
        if (p.stage === "downloading" && p.bytesTotal) {
          const pct = Math.round(
            (p.bytesReceived ?? 0) / p.bytesTotal * 100
          );
          progress.report({ message: `${p.message} (${pct}%)` });
        } else {
          progress.report({ message: p.message });
        }
      };
      return installToolchain(platform2, onProgress);
    }
  );
}
function notifyInstallSuccess(version, doctorWarnings) {
  if (doctorWarnings) {
    vscode2.window.showWarningMessage(
      `Inference toolchain v${version} installed, but doctor reported issues. See output for details.`,
      "Show Output"
    ).then((action) => {
      if (action === "Show Output") {
        vscode2.commands.executeCommand("inference.showOutput");
      }
    });
  } else {
    vscode2.window.showInformationMessage(
      `Inference toolchain v${version} installed successfully.`,
      "Show Output"
    ).then((action) => {
      if (action === "Show Output") {
        vscode2.commands.executeCommand("inference.showOutput");
      }
    });
  }
}
function notifyInstallError(errorMessage) {
  vscode2.window.showErrorMessage(
    `Inference toolchain installation failed: ${errorMessage}`,
    "Retry",
    "Download Manually",
    "Settings"
  ).then((action) => {
    if (action === "Retry") {
      vscode2.commands.executeCommand("inference.installToolchain");
    } else if (action === "Download Manually") {
      vscode2.env.openExternal(
        vscode2.Uri.parse(
          "https://github.com/Inferara/inference/releases"
        )
      );
    } else if (action === "Settings") {
      vscode2.commands.executeCommand(
        "workbench.action.openSettings",
        "inference.path"
      );
    }
  });
}

// src/commands/doctor.ts
var vscode4 = __toESM(require("vscode"));

// src/toolchain/doctor.ts
var STATUS_MAP = {
  OK: "ok",
  WARN: "warn",
  FAIL: "fail"
};
var CHECK_PATTERN = /^\s+\[(OK|WARN|FAIL)]\s+(.+?):\s+(.*)/;
function parseDoctorOutput(stdout) {
  const checks = [];
  const lines = stdout.split("\n");
  for (const line of lines) {
    const match = line.match(CHECK_PATTERN);
    if (match) {
      checks.push({
        status: STATUS_MAP[match[1]],
        name: match[2].trim(),
        message: match[3].trim()
      });
    }
  }
  let summary = "";
  for (let i = lines.length - 1; i >= 0; i--) {
    const trimmed = lines[i].trim();
    if (trimmed.length > 0 && !CHECK_PATTERN.test(lines[i])) {
      summary = trimmed;
      break;
    }
  }
  return {
    checks,
    hasErrors: checks.some((c) => c.status === "fail"),
    hasWarnings: checks.some((c) => c.status === "warn"),
    summary
  };
}
async function runDoctor(infsPath) {
  try {
    const result = await exec(infsPath, ["doctor"]);
    return parseDoctorOutput(result.stdout);
  } catch {
    return null;
  }
}

// src/ui/statusBar.ts
var vscode3 = __toESM(require("vscode"));
function createStatusBar() {
  const item = vscode3.window.createStatusBarItem(
    vscode3.StatusBarAlignment.Left,
    0
  );
  item.command = "inference.runDoctor";
  item.text = "$(loading~spin) Inference";
  item.tooltip = "Inference: Checking toolchain...";
  item.show();
  return item;
}
function updateStatusBar(item, result) {
  if (result === null) {
    item.text = "$(dash) Inference";
    item.tooltip = "Inference: Toolchain not found. Click to run doctor.";
    item.backgroundColor = void 0;
    return;
  }
  if (result.hasErrors) {
    item.text = "$(error) Inference";
    item.tooltip = `Inference: ${result.summary || "Toolchain errors detected"}`;
    item.backgroundColor = new vscode3.ThemeColor(
      "statusBarItem.errorBackground"
    );
    return;
  }
  if (result.hasWarnings) {
    item.text = "$(warning) Inference";
    item.tooltip = `Inference: ${result.summary || "Toolchain warnings detected"}`;
    item.backgroundColor = new vscode3.ThemeColor(
      "statusBarItem.warningBackground"
    );
    return;
  }
  item.text = "$(check) Inference";
  item.tooltip = "Inference: Toolchain healthy";
  item.backgroundColor = void 0;
}

// src/commands/doctor.ts
var running = false;
function registerDoctorCommand(outputChannel2, statusBarItem) {
  return vscode4.commands.registerCommand(
    "inference.runDoctor",
    async () => {
      if (running) {
        return;
      }
      const infsPath = detectInfs();
      if (!infsPath) {
        outputChannel2.appendLine("Doctor: infs binary not found.");
        updateStatusBar(statusBarItem, null);
        vscode4.window.showWarningMessage(
          "Inference toolchain not found. Install it first.",
          "Install"
        ).then((action) => {
          if (action === "Install") {
            vscode4.commands.executeCommand(
              "inference.installToolchain"
            );
          }
        });
        return;
      }
      running = true;
      try {
        outputChannel2.appendLine(
          `Running infs doctor (${infsPath})...`
        );
        const result = await runDoctor(infsPath);
        if (!result) {
          outputChannel2.appendLine(
            "Doctor: failed to execute infs doctor."
          );
          updateStatusBar(statusBarItem, null);
          vscode4.window.showErrorMessage(
            "Inference: Failed to run doctor. See output for details."
          );
          return;
        }
        formatDoctorOutput(outputChannel2, result);
        updateStatusBar(statusBarItem, result);
        if (result.hasErrors) {
          vscode4.window.showErrorMessage(
            `Inference doctor: ${result.summary}`,
            "Show Output"
          ).then((action) => {
            if (action === "Show Output") {
              outputChannel2.show();
            }
          });
        } else if (result.hasWarnings) {
          vscode4.window.showWarningMessage(
            `Inference doctor: ${result.summary}`,
            "Show Output"
          ).then((action) => {
            if (action === "Show Output") {
              outputChannel2.show();
            }
          });
        } else {
          vscode4.window.showInformationMessage(
            "Inference: Toolchain is healthy."
          );
        }
      } finally {
        running = false;
      }
    }
  );
}
var STATUS_TAGS = {
  ok: "[OK]  ",
  warn: "[WARN]",
  fail: "[FAIL]"
};
function formatDoctorOutput(outputChannel2, result) {
  outputChannel2.appendLine("--- Doctor Report ---");
  for (const check of result.checks) {
    const tag = STATUS_TAGS[check.status] ?? `[${check.status.toUpperCase()}]`;
    outputChannel2.appendLine(`  ${tag} ${check.name}: ${check.message}`);
  }
  if (result.summary) {
    outputChannel2.appendLine("");
    outputChannel2.appendLine(result.summary);
  }
  outputChannel2.appendLine("---------------------");
}

// src/commands/selectVersion.ts
var vscode5 = __toESM(require("vscode"));

// src/toolchain/versions.ts
function parseVersionsOutput(stdout) {
  try {
    const parsed = JSON.parse(stdout);
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed;
  } catch {
    return [];
  }
}
function parseCurrentVersion(stdout) {
  const match = stdout.match(/^infs\s+(\S+)/);
  return match ? match[1] : null;
}
async function fetchVersions(infsPath) {
  try {
    const result = await exec(infsPath, ["versions", "--json"]);
    if (result.exitCode !== 0) {
      return null;
    }
    return parseVersionsOutput(result.stdout);
  } catch {
    return null;
  }
}
async function getCurrentVersion(infsPath) {
  try {
    const result = await exec(infsPath, ["version"]);
    if (result.exitCode !== 0) {
      return null;
    }
    return parseCurrentVersion(result.stdout);
  } catch {
    return null;
  }
}
async function installAndSetDefault(infsPath, version) {
  const installResult = await exec(infsPath, ["install", version], {
    timeoutMs: 12e4
  });
  if (installResult.exitCode !== 0) {
    const detail = installResult.stderr || installResult.stdout;
    return { success: false, installedButNotDefault: false, error: detail };
  }
  const defaultResult = await exec(infsPath, ["default", version]);
  if (defaultResult.exitCode !== 0) {
    const detail = defaultResult.stderr || defaultResult.stdout;
    return { success: false, installedButNotDefault: true, error: detail };
  }
  return { success: true, installedButNotDefault: false };
}

// src/commands/selectVersion.ts
var selecting = false;
function registerSelectVersionCommand(outputChannel2) {
  return vscode5.commands.registerCommand(
    "inference.selectVersion",
    async () => {
      if (selecting) {
        vscode5.window.showInformationMessage(
          "Version selection is already in progress."
        );
        return;
      }
      const infsPath = detectInfs();
      if (!infsPath) {
        vscode5.window.showWarningMessage(
          "Inference toolchain not found. Install it first.",
          "Install"
        ).then((action) => {
          if (action === "Install") {
            vscode5.commands.executeCommand(
              "inference.installToolchain"
            );
          }
        });
        return;
      }
      selecting = true;
      try {
        const versions = await fetchVersions(infsPath);
        if (!versions) {
          vscode5.window.showErrorMessage(
            "Inference: Failed to fetch available versions."
          );
          return;
        }
        const currentVersion = await getCurrentVersion(infsPath);
        const available = versions.filter((v) => v.available_for_current).sort((a, b) => compareSemver(b.version, a.version));
        if (available.length === 0) {
          vscode5.window.showInformationMessage(
            "No toolchain versions available for this platform."
          );
          return;
        }
        const items = available.map((v) => {
          const tags = [];
          if (v.version === currentVersion) {
            tags.push("current");
          }
          if (v.stable) {
            tags.push("stable");
          }
          return {
            label: v.version,
            description: tags.length > 0 ? `(${tags.join(", ")})` : void 0
          };
        });
        if (currentVersion) {
          const idx = items.findIndex((i) => i.label === currentVersion);
          if (idx > 0) {
            const [item] = items.splice(idx, 1);
            items.unshift(item);
          }
        }
        const picked = await vscode5.window.showQuickPick(items, {
          placeHolder: "Select toolchain version",
          matchOnDescription: true
        });
        if (!picked) {
          return;
        }
        const selectedVersion = picked.label;
        if (selectedVersion === currentVersion) {
          vscode5.window.showInformationMessage(
            `Already using toolchain v${selectedVersion}.`
          );
          return;
        }
        await switchVersion(infsPath, selectedVersion, outputChannel2);
      } finally {
        selecting = false;
      }
    }
  );
}
async function switchVersion(infsPath, version, outputChannel2) {
  await vscode5.window.withProgress(
    {
      location: vscode5.ProgressLocation.Notification,
      title: "Inference Toolchain",
      cancellable: false
    },
    async (progress) => {
      progress.report({ message: `Switching to v${version}...` });
      outputChannel2.appendLine(`Switching to toolchain v${version}...`);
      const result = await installAndSetDefault(infsPath, version);
      if (result.success) {
        outputChannel2.appendLine(`Switched to toolchain v${version}.`);
        vscode5.window.showInformationMessage(
          `Switched to Inference toolchain v${version}.`,
          "Show Output"
        ).then((action) => {
          if (action === "Show Output") {
            outputChannel2.show();
          }
        });
        return;
      }
      outputChannel2.appendLine(`Version switch failed: ${result.error}`);
      if (result.installedButNotDefault) {
        vscode5.window.showWarningMessage(
          `Inference: v${version} was installed but could not be set as default. Run \`infs default ${version}\` manually.`,
          "Show Output"
        ).then((action) => {
          if (action === "Show Output") {
            outputChannel2.show();
          }
        });
      } else {
        vscode5.window.showErrorMessage(
          `Inference: Failed to install v${version}: ${result.error}`
        );
      }
    }
  );
}

// src/commands/update.ts
var vscode6 = __toESM(require("vscode"));
var updating = false;
function registerUpdateCommand(outputChannel2) {
  return vscode6.commands.registerCommand(
    "inference.updateToolchain",
    async () => {
      if (updating) {
        vscode6.window.showInformationMessage(
          "Update check is already in progress."
        );
        return;
      }
      const infsPath = detectInfs();
      if (!infsPath) {
        vscode6.window.showWarningMessage(
          "Inference toolchain not found. Install it first.",
          "Install"
        ).then((action) => {
          if (action === "Install") {
            vscode6.commands.executeCommand(
              "inference.installToolchain"
            );
          }
        });
        return;
      }
      updating = true;
      try {
        await checkForUpdatesImpl(infsPath, outputChannel2, true);
      } finally {
        updating = false;
      }
    }
  );
}
async function checkForUpdates(infsPath, outputChannel2) {
  const settings = getSettings();
  if (!settings.checkForUpdates) {
    return;
  }
  if (settings.channel === "none") {
    return;
  }
  await checkForUpdatesImpl(infsPath, outputChannel2, false);
}
async function checkForUpdatesImpl(infsPath, outputChannel2, userInitiated) {
  const currentVersion = await getCurrentVersion(infsPath);
  if (!currentVersion) {
    outputChannel2.appendLine("Update check: could not determine current version.");
    if (userInitiated) {
      vscode6.window.showErrorMessage(
        "Inference: Could not determine the current toolchain version."
      );
    }
    return;
  }
  outputChannel2.appendLine(`Update check: current version is ${currentVersion}.`);
  const versions = await fetchVersions(infsPath);
  if (!versions) {
    outputChannel2.appendLine("Update check: failed to fetch available versions.");
    if (userInitiated) {
      vscode6.window.showErrorMessage(
        "Inference: Failed to check for updates."
      );
    }
    return;
  }
  const settings = getSettings();
  const channel = settings.channel === "stable" || settings.channel === "latest" ? settings.channel : "stable";
  const candidates = versions.filter((v) => v.available_for_current).filter((v) => channel === "stable" ? v.stable : true);
  if (candidates.length === 0) {
    outputChannel2.appendLine("Update check: no versions available for this platform.");
    if (userInitiated) {
      vscode6.window.showInformationMessage(
        "Inference: No toolchain versions available for this platform."
      );
    }
    return;
  }
  const sorted = [...candidates].sort(
    (a, b) => compareSemver(b.version, a.version)
  );
  const latest = sorted[0];
  if (compareSemver(currentVersion, latest.version) >= 0) {
    outputChannel2.appendLine(
      `Update check: toolchain is up to date (v${currentVersion}).`
    );
    if (userInitiated) {
      vscode6.window.showInformationMessage(
        `Inference toolchain is up to date (v${currentVersion}).`
      );
    }
    return;
  }
  outputChannel2.appendLine(
    `Update check: v${latest.version} available (current: v${currentVersion}).`
  );
  const action = await vscode6.window.showInformationMessage(
    `Inference toolchain update available: v${latest.version} (current: v${currentVersion})`,
    "Update",
    "Release Notes"
  );
  if (action === "Update") {
    await performUpdate(infsPath, latest.version, outputChannel2);
  } else if (action === "Release Notes") {
    vscode6.env.openExternal(
      vscode6.Uri.parse(
        `https://github.com/Inferara/inference/releases/tag/v${latest.version}`
      )
    );
  }
}
async function performUpdate(infsPath, version, outputChannel2) {
  await vscode6.window.withProgress(
    {
      location: vscode6.ProgressLocation.Notification,
      title: "Inference Toolchain",
      cancellable: false
    },
    async (progress) => {
      progress.report({ message: `Updating to v${version}...` });
      outputChannel2.appendLine(`Updating to toolchain v${version}...`);
      const result = await installAndSetDefault(infsPath, version);
      if (result.success) {
        outputChannel2.appendLine(`Updated to toolchain v${version}.`);
        vscode6.window.showInformationMessage(
          `Inference toolchain updated to v${version}.`,
          "Show Output"
        ).then((action) => {
          if (action === "Show Output") {
            outputChannel2.show();
          }
        });
        return;
      }
      outputChannel2.appendLine(`Update failed: ${result.error}`);
      if (result.installedButNotDefault) {
        vscode6.window.showWarningMessage(
          `Inference: v${version} was installed but could not be set as default. Run \`infs default ${version}\` manually.`,
          "Show Output"
        ).then((action) => {
          if (action === "Show Output") {
            outputChannel2.show();
          }
        });
      } else {
        vscode6.window.showErrorMessage(
          `Inference: Failed to install v${version}: ${result.error}`
        );
      }
    }
  );
}

// src/extension.ts
var MIN_INFS_VERSION = "0.0.1-beta.1";
var outputChannel = vscode7.window.createOutputChannel("Inference");
function activate(context) {
  context.subscriptions.push(outputChannel);
  const statusBarItem = createStatusBar();
  context.subscriptions.push(statusBarItem);
  context.subscriptions.push(
    vscode7.commands.registerCommand("inference.showOutput", () => {
      outputChannel.show();
    })
  );
  context.subscriptions.push(registerInstallCommand(outputChannel));
  context.subscriptions.push(
    registerDoctorCommand(outputChannel, statusBarItem)
  );
  context.subscriptions.push(registerUpdateCommand(outputChannel));
  context.subscriptions.push(registerSelectVersionCommand(outputChannel));
  checkToolchain(statusBarItem).catch(
    (err) => outputChannel.appendLine(`Toolchain check failed: ${err}`)
  );
}
function deactivate() {
}
async function checkToolchain(statusBarItem) {
  const platform2 = detectPlatform();
  if (!platform2) {
    outputChannel.appendLine(
      `Unsupported platform: ${process.platform}-${process.arch}`
    );
    updateStatusBar(statusBarItem, null);
    vscode7.commands.executeCommand("setContext", "inference.toolchainInstalled", false);
    vscode7.window.showWarningMessage(
      `Inference: unsupported platform (${process.platform}-${process.arch}).`,
      "Download Page"
    ).then((action) => {
      if (action === "Download Page") {
        vscode7.env.openExternal(
          vscode7.Uri.parse(
            "https://github.com/Inferara/inference/releases"
          )
        );
      }
    });
    return;
  }
  outputChannel.appendLine(`Platform: ${platform2.id}`);
  const infsPath = detectInfs();
  if (!infsPath) {
    outputChannel.appendLine("infs binary not found.");
    updateStatusBar(statusBarItem, null);
    vscode7.commands.executeCommand("setContext", "inference.toolchainInstalled", false);
    const settings = getSettings();
    if (settings.autoInstall) {
      notifyMissing();
    }
    return;
  }
  outputChannel.appendLine(`infs found: ${infsPath}`);
  const versionOk = await checkInfsVersion(infsPath);
  if (!versionOk) {
    updateStatusBar(statusBarItem, null);
    vscode7.commands.executeCommand("setContext", "inference.toolchainInstalled", false);
    return;
  }
  outputChannel.appendLine("Toolchain detection complete.");
  vscode7.commands.executeCommand("setContext", "inference.toolchainInstalled", true);
  const doctorResult = await runDoctor(infsPath);
  updateStatusBar(statusBarItem, doctorResult);
  checkForUpdates(infsPath, outputChannel).catch(
    (err) => outputChannel.appendLine(`Update check failed: ${err}`)
  );
}
async function checkInfsVersion(infsPath) {
  try {
    const result = await exec(infsPath, ["version"]);
    if (result.exitCode !== 0) {
      outputChannel.appendLine(
        `infs version failed (exit ${result.exitCode}): ${result.stderr}`
      );
      return false;
    }
    const match = result.stdout.match(/^infs\s+(\S+)/);
    if (!match) {
      outputChannel.appendLine(
        `Could not parse infs version from: ${result.stdout.trim()}`
      );
      return false;
    }
    const version = match[1];
    outputChannel.appendLine(`infs version: ${version}`);
    if (compareSemver(version, MIN_INFS_VERSION) < 0) {
      outputChannel.appendLine(
        `infs version ${version} is below minimum ${MIN_INFS_VERSION}.`
      );
      vscode7.window.showWarningMessage(
        `Inference: infs version ${version} is outdated (minimum: ${MIN_INFS_VERSION}). Please update.`,
        "Update"
      );
      return false;
    }
    return true;
  } catch (err) {
    outputChannel.appendLine(`Failed to run infs version: ${err}`);
    return false;
  }
}
function notifyMissing() {
  vscode7.window.showInformationMessage(
    "Inference toolchain not found. Would you like to install it?",
    "Install",
    "Download Manually",
    "Configure Path"
  ).then((action) => {
    if (action === "Install") {
      vscode7.commands.executeCommand("inference.installToolchain");
    } else if (action === "Download Manually") {
      vscode7.env.openExternal(
        vscode7.Uri.parse(
          "https://github.com/Inferara/inference/releases"
        )
      );
    } else if (action === "Configure Path") {
      vscode7.commands.executeCommand(
        "workbench.action.openSettings",
        "inference.path"
      );
    }
  });
}
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  activate,
  deactivate
});
//# sourceMappingURL=extension.js.map
