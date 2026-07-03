"use strict";
var __create = Object.create;
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __getProtoOf = Object.getPrototypeOf;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __esm = (fn, res) => function __init() {
  return fn && (res = (0, fn[__getOwnPropNames(fn)[0]])(fn = 0)), res;
};
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

// src/toolchain/home.ts
function inferenceHome() {
  return process.env["INFERENCE_HOME"] || path.join(os2.homedir(), ".inference");
}
var os2, path;
var init_home = __esm({
  "src/toolchain/home.ts"() {
    "use strict";
    os2 = __toESM(require("os"));
    path = __toESM(require("path"));
  }
});

// src/utils/exec.ts
function exec(command, args, options) {
  const timeout = options?.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  return new Promise((resolve, reject) => {
    const child = cp.spawn(command, args, {
      cwd: options?.cwd,
      env: options?.env ? { ...process.env, ...options.env } : void 0,
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
var cp, DEFAULT_TIMEOUT_MS;
var init_exec = __esm({
  "src/utils/exec.ts"() {
    "use strict";
    cp = __toESM(require("child_process"));
    DEFAULT_TIMEOUT_MS = 3e4;
  }
});

// src/toolchain/doctor.ts
var doctor_exports = {};
__export(doctor_exports, {
  parseDoctorOutput: () => parseDoctorOutput,
  runDoctor: () => runDoctor
});
function parseDoctorOutput(stdout) {
  const checks = [];
  const lines = stdout.split(/\r?\n/);
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
    const binDir = path4.join(inferenceHome(), "bin");
    const sep = process.platform === "win32" ? ";" : ":";
    const augmentedPath = `${binDir}${sep}${process.env["PATH"] ?? ""}`;
    const result = await exec(infsPath, ["doctor"], {
      env: { PATH: augmentedPath }
    });
    return parseDoctorOutput(result.stdout);
  } catch (err) {
    console.error("infs doctor failed:", err);
    return null;
  }
}
var path4, STATUS_MAP, CHECK_PATTERN;
var init_doctor = __esm({
  "src/toolchain/doctor.ts"() {
    "use strict";
    path4 = __toESM(require("path"));
    init_exec();
    init_home();
    STATUS_MAP = {
      OK: "ok",
      WARN: "warn",
      FAIL: "fail"
    };
    CHECK_PATTERN = /^\s+\[(OK|WARN|FAIL)]\s+(.+?):\s+(.*)/;
  }
});

// src/extension.ts
var extension_exports = {};
__export(extension_exports, {
  activate: () => activate,
  applyTerminalPath: () => applyTerminalPath,
  deactivate: () => deactivate
});
module.exports = __toCommonJS(extension_exports);
var vscode10 = __toESM(require("vscode"));
var path6 = __toESM(require("path"));

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
var path2 = __toESM(require("path"));

// src/config/settings.ts
var vscode = __toESM(require("vscode"));
function getSettings() {
  const config = vscode.workspace.getConfiguration("inference");
  return {
    path: config.get("path", ""),
    autoInstall: config.get("autoInstall", true),
    checkForUpdates: config.get("checkForUpdates", true)
  };
}

// src/toolchain/detection.ts
init_home();
function isExecutable(filePath) {
  try {
    const mode = process.platform === "win32" ? fs.constants.F_OK : fs.constants.X_OK;
    fs.accessSync(filePath, mode);
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
    const candidate = path2.join(dir, binaryName);
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
      return { path: settings.path, source: "settings" };
    }
    return null;
  }
  const managedPath = path2.join(inferenceHome(), "bin", binaryName);
  if (isExecutable(managedPath)) {
    return { path: managedPath, source: "managed" };
  }
  const pathResult = findInPath(binaryName);
  if (pathResult) {
    return { path: pathResult, source: "path" };
  }
  return null;
}

// src/extension.ts
init_exec();

// src/utils/semver.ts
function compareSemver(a, b) {
  const clean = (v) => v.replace(/^v/i, "");
  const [coreA, preA] = clean(a).split("-", 2);
  const [coreB, preB] = clean(b).split("-", 2);
  const pa = coreA.split(".").map(Number);
  const pb = coreB.split(".").map(Number);
  for (let i = 0; i < 3; i++) {
    const diff = (pa[i] || 0) - (pb[i] || 0);
    if (diff !== 0) {
      return diff;
    }
  }
  if (!preA && !preB) {
    return 0;
  }
  if (preA && !preB) {
    return -1;
  }
  if (!preA && preB) {
    return 1;
  }
  const partsA = preA.split(".");
  const partsB = preB.split(".");
  const len = Math.max(partsA.length, partsB.length);
  for (let i = 0; i < len; i++) {
    if (i >= partsA.length) {
      return -1;
    }
    if (i >= partsB.length) {
      return 1;
    }
    const numA = Number(partsA[i]);
    const numB = Number(partsB[i]);
    const aIsNum = !Number.isNaN(numA);
    const bIsNum = !Number.isNaN(numB);
    if (aIsNum && bIsNum) {
      if (numA !== numB) {
        return numA - numB;
      }
    } else if (aIsNum) {
      return -1;
    } else if (bIsNum) {
      return 1;
    } else {
      if (partsA[i] < partsB[i]) {
        return -1;
      }
      if (partsA[i] > partsB[i]) {
        return 1;
      }
    }
  }
  return 0;
}

// src/commands/install.ts
var vscode3 = __toESM(require("vscode"));

// src/toolchain/installation.ts
var fs4 = __toESM(require("fs"));
var os3 = __toESM(require("os"));
var path5 = __toESM(require("path"));
init_home();

// src/utils/download.ts
var https = __toESM(require("https"));
var http = __toESM(require("http"));
var fs2 = __toESM(require("fs"));
var crypto = __toESM(require("crypto"));
var DEFAULT_TIMEOUT_MS2 = 15e3;
var MAX_REDIRECTS = 5;
var SOCKET_TIMEOUT_MS = 15e3;
var MAX_JSON_RESPONSE_BYTES = 10 * 1024 * 1024;
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
        let totalBytes = 0;
        res.on("data", (chunk) => {
          totalBytes += chunk.length;
          if (totalBytes > MAX_JSON_RESPONSE_BYTES) {
            res.destroy();
            reject(new Error(`Response too large (>${MAX_JSON_RESPONSE_BYTES} bytes) from ${url}`));
            return;
          }
          chunks.push(chunk);
        });
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
    let settled = false;
    const settle = (fn, ...args) => {
      if (!settled) {
        settled = true;
        fn(...args);
      }
    };
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
        let dataTimer;
        const clearDataTimer = () => {
          if (dataTimer) {
            clearTimeout(dataTimer);
            dataTimer = void 0;
          }
        };
        const resetTimer = () => {
          clearDataTimer();
          dataTimer = setTimeout(() => {
            res.destroy();
            ws.destroy();
            cleanup();
            settle(reject, new Error(`Download timed out for ${url}`));
          }, timeout);
        };
        resetTimer();
        res.on("data", resetTimer);
        res.on("end", clearDataTimer);
        ws.on("finish", () => {
          clearDataTimer();
          try {
            fs2.renameSync(partialPath, options.destPath);
            settle(resolve);
          } catch (err) {
            cleanup();
            settle(
              reject,
              new Error(
                `Failed to save download to ${options.destPath}: ${err instanceof Error ? err.message : err}`
              )
            );
          }
        });
        ws.on("error", (err) => {
          clearDataTimer();
          res.destroy();
          cleanup();
          settle(
            reject,
            new Error(
              `Failed to write download: ${err.message}`
            )
          );
        });
        res.on("error", (err) => {
          clearDataTimer();
          ws.destroy();
          cleanup();
          settle(
            reject,
            new Error(
              `Download stream error from ${url}: ${err.message}`
            )
          );
        });
      },
      (err) => settle(reject, err)
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
var path3 = __toESM(require("path"));
init_exec();
async function extractArchive(options) {
  fs3.mkdirSync(options.destDir, { recursive: true });
  if (options.archivePath.endsWith(".tar.gz") || options.archivePath.endsWith(".tgz")) {
    await extractTarGz(options.archivePath, options.destDir);
  } else if (options.archivePath.endsWith(".zip")) {
    await extractZip(options.archivePath, options.destDir);
  } else {
    throw new Error(
      `Unsupported archive format: ${path3.basename(options.archivePath)}`
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
function escapePowerShellSingleQuote(value) {
  return value.replace(/'/g, "''");
}
async function extractZip(archivePath, destDir) {
  const safePath = escapePowerShellSingleQuote(archivePath);
  const safeDest = escapePowerShellSingleQuote(destDir);
  const result = await exec("powershell", [
    "-NoProfile",
    "-Command",
    `Expand-Archive -LiteralPath '${safePath}' -DestinationPath '${safeDest}' -Force`
  ]);
  if (result.exitCode !== 0) {
    throw new Error(
      `zip extraction failed (exit ${result.exitCode}): ${result.stderr}`
    );
  }
}
function setExecutablePermissions(dir) {
  let entries;
  try {
    entries = fs3.readdirSync(dir, { withFileTypes: true });
  } catch {
    return;
  }
  for (const entry of entries) {
    if (entry.isFile()) {
      try {
        fs3.chmodSync(path3.join(dir, entry.name), 493);
      } catch {
      }
    }
  }
}

// src/toolchain/installation.ts
init_exec();

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
function findLatestRelease(manifest, platform2) {
  if (manifest.length === 0) {
    return null;
  }
  const sorted = [...manifest].sort(
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
var DEFAULT_DIST_SERVER = "https://inference-lang.org";
var RELEASES_PATH = "/releases.json";
function manifestUrl() {
  const server = process.env["INFS_DIST_SERVER"]?.trim();
  const base = server && server.length > 0 ? server.replace(/\/+$/, "") : DEFAULT_DIST_SERVER;
  return `${base}${RELEASES_PATH}`;
}
async function installToolchain(platform2, onProgress) {
  onProgress?.({
    stage: "fetching-manifest",
    message: "Fetching release manifest..."
  });
  const manifest = await fetchJson(manifestUrl());
  if (!Array.isArray(manifest)) {
    throw new Error("Invalid release manifest: expected an array.");
  }
  for (const entry of manifest) {
    if (typeof entry?.version !== "string" || typeof entry?.stable !== "boolean" || !Array.isArray(entry?.files)) {
      throw new Error(
        `Invalid release manifest entry: ${JSON.stringify(entry)?.slice(0, 200)}`
      );
    }
  }
  const match = findLatestRelease(manifest, platform2);
  if (!match) {
    throw new Error(
      `No compatible infs release found for ${platform2.id}.`
    );
  }
  const { release, fileUrl, sha256 } = match;
  const version = release.version;
  onProgress?.({
    stage: "downloading",
    message: `Downloading infs v${version}...`
  });
  const destDir = path5.join(inferenceHome(), "bin");
  fs4.mkdirSync(destDir, { recursive: true });
  const archiveName = `infs-${platform2.id}${platform2.archiveExtension}`;
  const tmpDir = fs4.mkdtempSync(path5.join(os3.tmpdir(), "infs-"));
  const archivePath = path5.join(tmpDir, archiveName);
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
      fs4.rmSync(tmpDir, { recursive: true, force: true });
    } catch {
    }
  }
  const infsPath = path5.join(destDir, platform2.binaryName);
  if (!fs4.existsSync(infsPath)) {
    throw new Error(
      `infs binary not found at ${infsPath} after extraction.`
    );
  }
  onProgress?.({
    stage: "installing",
    message: "Running infs install..."
  });
  const binDir = path5.join(inferenceHome(), "bin");
  const sep = process.platform === "win32" ? ";" : ":";
  const augmentedPath = `${binDir}${sep}${process.env["PATH"] ?? ""}`;
  const installResult = await exec(infsPath, ["install"], {
    timeoutMs: 12e4,
    env: { PATH: augmentedPath }
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
      timeoutMs: 3e4,
      env: { PATH: augmentedPath }
    });
    if (doctorResult.exitCode !== 0) {
      doctorWarnings = true;
    } else {
      const { parseDoctorOutput: parseDoctorOutput2 } = await Promise.resolve().then(() => (init_doctor(), doctor_exports));
      const parsed = parseDoctorOutput2(doctorResult.stdout);
      if (parsed.hasErrors || parsed.hasWarnings) {
        doctorWarnings = true;
      }
    }
  } catch {
    doctorWarnings = true;
  }
  return { infsPath, version, doctorWarnings };
}

// src/commands/install.ts
init_doctor();

// src/ui/statusBar.ts
var vscode2 = __toESM(require("vscode"));

// src/ui/statusBarState.ts
function determineStatusBarState(result) {
  if (result === null) {
    return {
      icon: "dash",
      label: "Inference",
      tooltip: "Inference: Toolchain not found. Click to run doctor.",
      background: "none"
    };
  }
  if (result.hasErrors) {
    return {
      icon: "error",
      label: "Inference",
      tooltip: `Inference: ${result.summary || "Toolchain errors detected"}`,
      background: "error"
    };
  }
  if (result.hasWarnings) {
    return {
      icon: "warning",
      label: "Inference",
      tooltip: `Inference: ${result.summary || "Toolchain warnings detected"}`,
      background: "warning"
    };
  }
  return {
    icon: "check",
    label: "Inference",
    tooltip: "Inference: Toolchain healthy",
    background: "none"
  };
}

// src/ui/statusBar.ts
var ICON_MAP = {
  loading: "$(loading~spin)",
  dash: "$(dash)",
  check: "$(check)",
  warning: "$(warning)",
  error: "$(error)"
};
var BACKGROUND_MAP = {
  none: void 0,
  warning: new vscode2.ThemeColor("statusBarItem.warningBackground"),
  error: new vscode2.ThemeColor("statusBarItem.errorBackground")
};
function createStatusBar() {
  const item = vscode2.window.createStatusBarItem(
    vscode2.StatusBarAlignment.Left,
    0
  );
  item.command = "inference.runDoctor";
  item.text = "$(loading~spin) Inference";
  item.tooltip = "Inference: Checking toolchain...";
  item.show();
  return item;
}
function updateStatusBar(item, result) {
  const state = determineStatusBarState(result);
  item.text = `${ICON_MAP[state.icon]} ${state.label}`;
  item.tooltip = state.tooltip;
  item.backgroundColor = BACKGROUND_MAP[state.background];
}

// src/commands/install.ts
var installing = false;
function registerInstallCommand(outputChannel2, statusBarItem) {
  return vscode3.commands.registerCommand(
    "inference.installToolchain",
    async () => {
      if (installing) {
        vscode3.window.showInformationMessage(
          "Inference toolchain installation is already in progress."
        );
        return;
      }
      const platform2 = detectPlatform();
      if (!platform2) {
        vscode3.window.showErrorMessage(
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
        vscode3.commands.executeCommand(
          "setContext",
          "inference.toolchainInstalled",
          true
        );
        const doctorResult = await runDoctor(result.infsPath);
        updateStatusBar(statusBarItem, doctorResult);
        vscode3.commands.executeCommand("inference.refreshConfigView");
        vscode3.commands.executeCommand("inference.applyTerminalPath");
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
  return vscode3.window.withProgress(
    {
      location: vscode3.ProgressLocation.Notification,
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
    vscode3.window.showWarningMessage(
      `Inference toolchain v${version} installed, but doctor reported issues. See output for details.`,
      "Show Output"
    ).then((action) => {
      if (action === "Show Output") {
        vscode3.commands.executeCommand("inference.showOutput");
      }
    });
  } else {
    vscode3.window.showInformationMessage(
      `Inference toolchain v${version} installed successfully.`
    );
  }
}
function notifyInstallError(errorMessage) {
  vscode3.window.showErrorMessage(
    `Inference toolchain installation failed: ${errorMessage}`,
    "Retry",
    "Download Manually",
    "Settings"
  ).then((action) => {
    if (action === "Retry") {
      vscode3.commands.executeCommand("inference.installToolchain");
    } else if (action === "Download Manually") {
      vscode3.env.openExternal(
        vscode3.Uri.parse(
          "https://github.com/Inferara/inference/releases"
        )
      );
    } else if (action === "Settings") {
      vscode3.commands.executeCommand(
        "workbench.action.openSettings",
        "inference.path"
      );
    }
  });
}

// src/commands/installComponent.ts
var vscode4 = __toESM(require("vscode"));
init_exec();

// src/toolchain/components.ts
var KNOWN_COMPONENTS = ["wasm-opt"];
function componentAddArgs(component) {
  return ["component", "add", component];
}
function wasmOptNeedsAttention(result) {
  return result.checks.some(
    (check) => check.name === "wasm-opt" && (check.status === "warn" || check.status === "fail")
  );
}

// src/commands/installComponent.ts
var installing2 = false;
var INSTALL_TIMEOUT_MS = 6e5;
function registerInstallComponentCommand(outputChannel2) {
  return vscode4.commands.registerCommand(
    "inference.installComponent",
    async (component = "wasm-opt") => {
      if (installing2) {
        vscode4.window.showInformationMessage(
          "Inference component installation is already in progress."
        );
        return;
      }
      if (!isKnownComponent(component)) {
        vscode4.window.showErrorMessage(
          `Inference: unknown component '${component}'.`
        );
        return;
      }
      const detection = detectInfs();
      if (!detection) {
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
      installing2 = true;
      try {
        const result = await installWithProgress2(
          detection.path,
          component,
          outputChannel2
        );
        if (result.stdout) {
          outputChannel2.appendLine(result.stdout);
        }
        if (result.stderr) {
          outputChannel2.appendLine(result.stderr);
        }
        if (result.exitCode === 0) {
          vscode4.window.showInformationMessage(
            `Inference: component '${component}' installed.`
          );
          vscode4.commands.executeCommand("inference.runDoctor");
        } else {
          notifyInstallError2(component);
        }
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        outputChannel2.appendLine(
          `Component installation failed: ${message}`
        );
        notifyInstallError2(component);
      } finally {
        installing2 = false;
      }
    }
  );
}
function isKnownComponent(name) {
  return KNOWN_COMPONENTS.includes(name);
}
function installWithProgress2(infsPath, component, outputChannel2) {
  return vscode4.window.withProgress(
    {
      location: vscode4.ProgressLocation.Notification,
      title: "Inference Component",
      cancellable: false
    },
    async (progress) => {
      progress.report({ message: `Installing ${component}...` });
      outputChannel2.appendLine(`Installing component '${component}'...`);
      return exec(infsPath, componentAddArgs(component), {
        timeoutMs: INSTALL_TIMEOUT_MS
      });
    }
  );
}
function notifyInstallError2(component) {
  vscode4.window.showErrorMessage(
    `Inference: failed to install component '${component}'. See output for details.`,
    "Show Output",
    "Retry"
  ).then((action) => {
    if (action === "Show Output") {
      vscode4.commands.executeCommand("inference.showOutput");
    } else if (action === "Retry") {
      vscode4.commands.executeCommand(
        "inference.installComponent",
        component
      );
    }
  });
}

// src/commands/doctor.ts
var vscode5 = __toESM(require("vscode"));
init_doctor();

// src/toolchain/doctorFormat.ts
var STATUS_TAGS = {
  ok: "[OK]  ",
  warn: "[WARN]",
  fail: "[FAIL]"
};
function formatDoctorChecks(result) {
  const lines = [];
  lines.push("--- Doctor Report ---");
  for (const check of result.checks) {
    const tag = STATUS_TAGS[check.status] ?? `[${check.status.toUpperCase()}]`;
    lines.push(`  ${tag} ${check.name}: ${check.message}`);
  }
  if (result.summary) {
    lines.push("");
    lines.push(result.summary);
  }
  lines.push("---------------------");
  return lines;
}

// src/commands/doctor.ts
var running = false;
function registerDoctorCommand(outputChannel2, statusBarItem) {
  return vscode5.commands.registerCommand(
    "inference.runDoctor",
    async () => {
      if (running) {
        return;
      }
      const detection = detectInfs();
      if (!detection) {
        outputChannel2.appendLine("Doctor: infs binary not found.");
        updateStatusBar(statusBarItem, null);
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
      running = true;
      try {
        outputChannel2.appendLine(
          `Running infs doctor (${detection.path})...`
        );
        const result = await runDoctor(detection.path);
        if (!result) {
          outputChannel2.appendLine(
            "Doctor: failed to execute infs doctor."
          );
          updateStatusBar(statusBarItem, null);
          vscode5.window.showErrorMessage(
            "Inference: Failed to run doctor. See output for details."
          );
          return;
        }
        for (const line of formatDoctorChecks(result)) {
          outputChannel2.appendLine(line);
        }
        updateStatusBar(statusBarItem, result);
        vscode5.commands.executeCommand("inference.refreshConfigView");
        if (result.hasErrors) {
          const actions = ["Show Output"];
          if (wasmOptNeedsAttention(result)) {
            actions.push("Install wasm-opt");
          }
          vscode5.window.showErrorMessage(
            `Inference doctor: ${result.summary}`,
            ...actions
          ).then((action) => {
            if (action === "Show Output") {
              outputChannel2.show();
            } else if (action === "Install wasm-opt") {
              vscode5.commands.executeCommand(
                "inference.installComponent",
                "wasm-opt"
              );
            }
          });
        } else if (result.hasWarnings) {
          const actions = ["Show Output"];
          if (wasmOptNeedsAttention(result)) {
            actions.push("Install wasm-opt");
          }
          vscode5.window.showWarningMessage(
            `Inference doctor: ${result.summary}`,
            ...actions
          ).then((action) => {
            if (action === "Show Output") {
              outputChannel2.show();
            } else if (action === "Install wasm-opt") {
              vscode5.commands.executeCommand(
                "inference.installComponent",
                "wasm-opt"
              );
            }
          });
        } else {
          vscode5.window.showInformationMessage(
            "Inference: Toolchain is healthy."
          );
        }
      } finally {
        running = false;
      }
    }
  );
}

// src/commands/selectVersion.ts
var vscode7 = __toESM(require("vscode"));

// src/toolchain/versions.ts
init_exec();
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
    const result = await exec(infsPath, ["versions", "--json"], {
      timeoutMs: 3e4
    });
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
    const result = await exec(infsPath, ["version"], {
      timeoutMs: 1e4
    });
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
  const defaultResult = await exec(infsPath, ["default", version], {
    timeoutMs: 3e4
  });
  if (defaultResult.exitCode !== 0) {
    const detail = defaultResult.stderr || defaultResult.stdout;
    return { success: false, installedButNotDefault: true, error: detail };
  }
  return { success: true, installedButNotDefault: false };
}

// src/toolchain/versionPicker.ts
function buildVersionPickItems(versions, currentVersion) {
  const available = versions.filter((v) => v.available_for_current).sort((a, b) => compareSemver(b.version, a.version));
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
  return items;
}

// src/commands/versionChange.ts
var vscode6 = __toESM(require("vscode"));
async function performVersionChange(infsPath, version, outputChannel2, actionVerb) {
  await vscode6.window.withProgress(
    {
      location: vscode6.ProgressLocation.Notification,
      title: "Inference Toolchain",
      cancellable: false
    },
    async (progress) => {
      progress.report({ message: `${actionVerb} v${version}...` });
      outputChannel2.appendLine(`${actionVerb} toolchain v${version}...`);
      const result = await installAndSetDefault(infsPath, version);
      if (result.success) {
        outputChannel2.appendLine(
          `${actionVerb} toolchain v${version} complete.`
        );
        vscode6.commands.executeCommand(
          "setContext",
          "inference.toolchainInstalled",
          true
        );
        vscode6.commands.executeCommand("inference.applyTerminalPath");
        vscode6.commands.executeCommand("inference.runDoctor");
        vscode6.window.showInformationMessage(
          `Inference toolchain ${actionVerb.toLowerCase()} to v${version}.`,
          "Show Output"
        ).then((action) => {
          if (action === "Show Output") {
            outputChannel2.show();
          }
        });
        return;
      }
      outputChannel2.appendLine(
        `${actionVerb} failed: ${result.error}`
      );
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

// src/commands/selectVersion.ts
var selecting = false;
function registerSelectVersionCommand(outputChannel2) {
  return vscode7.commands.registerCommand(
    "inference.selectVersion",
    async () => {
      if (selecting) {
        vscode7.window.showInformationMessage(
          "Version selection is already in progress."
        );
        return;
      }
      const detection = detectInfs();
      if (!detection) {
        vscode7.window.showWarningMessage(
          "Inference toolchain not found. Install it first.",
          "Install"
        ).then((action) => {
          if (action === "Install") {
            vscode7.commands.executeCommand(
              "inference.installToolchain"
            );
          }
        });
        return;
      }
      selecting = true;
      try {
        const versions = await fetchVersions(detection.path);
        if (!versions) {
          vscode7.window.showErrorMessage(
            "Inference: Failed to fetch available versions."
          );
          return;
        }
        const currentVersion = await getCurrentVersion(detection.path);
        const items = buildVersionPickItems(versions, currentVersion);
        if (items.length === 0) {
          vscode7.window.showInformationMessage(
            "No toolchain versions available for this platform."
          );
          return;
        }
        const picked = await vscode7.window.showQuickPick(items, {
          placeHolder: "Select toolchain version",
          matchOnDescription: true
        });
        if (!picked) {
          return;
        }
        const selectedVersion = picked.label;
        if (selectedVersion === currentVersion) {
          vscode7.window.showInformationMessage(
            `Already using toolchain v${selectedVersion}.`
          );
          return;
        }
        await performVersionChange(detection.path, selectedVersion, outputChannel2, "Switching to");
      } finally {
        selecting = false;
      }
    }
  );
}

// src/commands/update.ts
var vscode8 = __toESM(require("vscode"));

// src/toolchain/updateCheck.ts
function checkUpdateAvailable(currentVersion, versions) {
  if (!currentVersion) {
    return { status: "no-current-version" };
  }
  if (!versions) {
    return { status: "no-versions" };
  }
  const candidates = versions.filter((v) => v.available_for_current);
  if (candidates.length === 0) {
    return { status: "no-versions" };
  }
  const sorted = [...candidates].sort(
    (a, b) => compareSemver(b.version, a.version)
  );
  const latest = sorted[0];
  if (compareSemver(currentVersion, latest.version) >= 0) {
    return { status: "up-to-date", version: currentVersion };
  }
  return {
    status: "update-available",
    current: currentVersion,
    latest: latest.version
  };
}

// src/commands/update.ts
var updating = false;
function registerUpdateCommand(outputChannel2) {
  return vscode8.commands.registerCommand(
    "inference.updateToolchain",
    async () => {
      if (updating) {
        vscode8.window.showInformationMessage(
          "Update check is already in progress."
        );
        return;
      }
      const detection = detectInfs();
      if (!detection) {
        vscode8.window.showWarningMessage(
          "Inference toolchain not found. Install it first.",
          "Install"
        ).then((action) => {
          if (action === "Install") {
            vscode8.commands.executeCommand(
              "inference.installToolchain"
            );
          }
        });
        return;
      }
      updating = true;
      try {
        await checkForUpdatesImpl(detection.path, outputChannel2, true);
      } finally {
        updating = false;
      }
    }
  );
}
async function checkForUpdates(infsPath, outputChannel2) {
  if (updating) {
    return;
  }
  const settings = getSettings();
  if (!settings.checkForUpdates) {
    return;
  }
  updating = true;
  try {
    await checkForUpdatesImpl(infsPath, outputChannel2, false);
  } finally {
    updating = false;
  }
}
async function checkForUpdatesImpl(infsPath, outputChannel2, userInitiated) {
  const currentVersion = await getCurrentVersion(infsPath);
  if (!currentVersion) {
    outputChannel2.appendLine("Update check: could not determine current version.");
    if (userInitiated) {
      vscode8.window.showErrorMessage(
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
      vscode8.window.showErrorMessage(
        "Inference: Failed to check for updates."
      );
    }
    return;
  }
  const result = checkUpdateAvailable(currentVersion, versions);
  switch (result.status) {
    case "no-current-version":
      outputChannel2.appendLine("Update check: could not determine current version.");
      if (userInitiated) {
        vscode8.window.showErrorMessage(
          "Inference: Could not determine the current toolchain version."
        );
      }
      return;
    case "no-versions":
      outputChannel2.appendLine("Update check: no versions available for this platform.");
      if (userInitiated) {
        vscode8.window.showInformationMessage(
          "Inference: No toolchain versions available for this platform."
        );
      }
      return;
    case "up-to-date":
      outputChannel2.appendLine(
        `Update check: toolchain is up to date (v${result.version}).`
      );
      if (userInitiated) {
        vscode8.window.showInformationMessage(
          `Inference toolchain is up to date (v${result.version}).`
        );
      }
      return;
    case "update-available": {
      outputChannel2.appendLine(
        `Update check: v${result.latest} available (current: v${result.current}).`
      );
      const action = await vscode8.window.showInformationMessage(
        `Inference toolchain update available: v${result.latest} (current: v${result.current})`,
        "Update",
        "Release Notes"
      );
      if (action === "Update") {
        await performVersionChange(infsPath, result.latest, outputChannel2, "Updating to");
      } else if (action === "Release Notes") {
        vscode8.env.openExternal(
          vscode8.Uri.parse(
            `https://github.com/Inferara/inference/releases/tag/v${result.latest}`
          )
        );
      }
      return;
    }
  }
}

// src/ui/configTree.ts
var vscode9 = __toESM(require("vscode"));
init_home();
init_exec();
var ConfigItem = class extends vscode9.TreeItem {
  constructor(label, kind, collapsible, groupId, settingKey, copyValue) {
    super(label, collapsible);
    this.kind = kind;
    this.groupId = groupId;
    this.settingKey = settingKey;
    this.copyValue = copyValue;
    if (kind === "group") {
      this.iconPath = new vscode9.ThemeIcon(
        groupId === "toolchain" ? "tools" : "gear"
      );
    }
    if (settingKey) {
      this.command = {
        title: "Open Setting",
        command: "workbench.action.openSettings",
        arguments: [settingKey]
      };
    }
    if (copyValue) {
      this.contextValue = "inference.configPath";
    }
  }
  kind;
  groupId;
  settingKey;
  copyValue;
};
var InferenceConfigProvider = class {
  _onDidChangeTreeData = new vscode9.EventEmitter();
  onDidChangeTreeData = this._onDidChangeTreeData.event;
  detection = null;
  version = null;
  doctorResult = null;
  refresh(detection, doctorResult) {
    if (detection !== void 0) {
      this.detection = detection;
    }
    if (doctorResult !== void 0) {
      this.doctorResult = doctorResult;
    }
    this._onDidChangeTreeData.fire(void 0);
  }
  getTreeItem(element) {
    return element;
  }
  async getChildren(element) {
    if (!element) {
      return [
        new ConfigItem(
          "Toolchain",
          "group",
          vscode9.TreeItemCollapsibleState.Expanded,
          "toolchain"
        ),
        new ConfigItem(
          "Settings",
          "group",
          vscode9.TreeItemCollapsibleState.Expanded,
          "settings"
        )
      ];
    }
    if (element.groupId === "toolchain") {
      return this.getToolchainChildren();
    }
    if (element.groupId === "settings") {
      return this.getSettingsChildren();
    }
    return [];
  }
  async getToolchainChildren() {
    const detection = this.detection ?? detectInfs();
    const items = [];
    if (!detection) {
      const item = new ConfigItem(
        "infs: not found",
        "property",
        vscode9.TreeItemCollapsibleState.None
      );
      item.iconPath = new vscode9.ThemeIcon("error");
      item.command = {
        title: "Install Toolchain",
        command: "inference.installToolchain",
        arguments: []
      };
      items.push(item);
      return items;
    }
    const infsItem = new ConfigItem(
      `infs: ${detection.path}  (${detection.source})`,
      "property",
      vscode9.TreeItemCollapsibleState.None,
      void 0,
      void 0,
      detection.path
    );
    infsItem.iconPath = new vscode9.ThemeIcon("file-binary");
    items.push(infsItem);
    const version = await this.resolveVersion(detection.path);
    const versionItem = new ConfigItem(
      `Version: ${version ?? "unknown"}`,
      "property",
      vscode9.TreeItemCollapsibleState.None
    );
    versionItem.iconPath = new vscode9.ThemeIcon("tag");
    items.push(versionItem);
    const home = inferenceHome();
    const homeIsDefault = !process.env["INFERENCE_HOME"];
    const homeItem = new ConfigItem(
      `Home: ${home}  (${homeIsDefault ? "default" : "env"})`,
      "property",
      vscode9.TreeItemCollapsibleState.None,
      void 0,
      void 0,
      home
    );
    homeItem.iconPath = new vscode9.ThemeIcon("home");
    items.push(homeItem);
    const platform2 = detectPlatform();
    const platformItem = new ConfigItem(
      `Platform: ${platform2?.id ?? "unknown"}`,
      "property",
      vscode9.TreeItemCollapsibleState.None
    );
    platformItem.iconPath = new vscode9.ThemeIcon("device-desktop");
    items.push(platformItem);
    const status = this.doctorResult ? this.doctorResult.hasErrors ? "errors" : this.doctorResult.hasWarnings ? "warnings" : "healthy" : "unknown";
    const statusIcon = this.doctorResult ? this.doctorResult.hasErrors ? "error" : this.doctorResult.hasWarnings ? "warning" : "pass" : "question";
    const statusItem = new ConfigItem(
      `Status: ${status}`,
      "property",
      vscode9.TreeItemCollapsibleState.None
    );
    statusItem.iconPath = new vscode9.ThemeIcon(statusIcon);
    statusItem.command = {
      title: "Run Doctor",
      command: "inference.runDoctor",
      arguments: []
    };
    items.push(statusItem);
    return items;
  }
  getSettingsChildren() {
    const settings = getSettings();
    const pathItem = new ConfigItem(
      `Path: ${settings.path || "(auto-detect)"}`,
      "property",
      vscode9.TreeItemCollapsibleState.None,
      void 0,
      "inference.path"
    );
    pathItem.iconPath = new vscode9.ThemeIcon("file-symlink-directory");
    const autoInstallItem = new ConfigItem(
      `Auto Install: ${settings.autoInstall ? "enabled" : "disabled"}`,
      "property",
      vscode9.TreeItemCollapsibleState.None,
      void 0,
      "inference.autoInstall"
    );
    autoInstallItem.iconPath = new vscode9.ThemeIcon("cloud-download");
    const updateItem = new ConfigItem(
      `Check for Updates: ${settings.checkForUpdates ? "enabled" : "disabled"}`,
      "property",
      vscode9.TreeItemCollapsibleState.None,
      void 0,
      "inference.checkForUpdates"
    );
    updateItem.iconPath = new vscode9.ThemeIcon("sync");
    return [pathItem, autoInstallItem, updateItem];
  }
  async resolveVersion(infsPath) {
    if (this.version) {
      return this.version;
    }
    try {
      const result = await exec(infsPath, ["version"]);
      if (result.exitCode !== 0) {
        return null;
      }
      const match = result.stdout.match(/^infs\s+(\S+)/);
      if (match) {
        this.version = match[1];
        return this.version;
      }
      return null;
    } catch {
      return null;
    }
  }
  dispose() {
    this._onDidChangeTreeData.dispose();
  }
};

// src/extension.ts
init_doctor();
var MIN_INFS_VERSION = "0.0.1-beta.1";
var outputChannel = vscode10.window.createOutputChannel("Inference", { log: true });
function activate(context) {
  context.subscriptions.push(outputChannel);
  const statusBarItem = createStatusBar();
  context.subscriptions.push(statusBarItem);
  context.subscriptions.push(
    vscode10.commands.registerCommand("inference.showOutput", () => {
      outputChannel.show();
    })
  );
  context.subscriptions.push(registerInstallCommand(outputChannel, statusBarItem));
  context.subscriptions.push(
    registerDoctorCommand(outputChannel, statusBarItem)
  );
  context.subscriptions.push(registerInstallComponentCommand(outputChannel));
  context.subscriptions.push(registerUpdateCommand(outputChannel));
  context.subscriptions.push(registerSelectVersionCommand(outputChannel));
  const configProvider = new InferenceConfigProvider();
  const configView = vscode10.window.createTreeView("inference.configView", {
    treeDataProvider: configProvider
  });
  context.subscriptions.push(configView);
  context.subscriptions.push(configProvider);
  context.subscriptions.push(
    vscode10.commands.registerCommand("inference.refreshConfigView", () => {
      configProvider.refresh();
    })
  );
  context.subscriptions.push(
    vscode10.commands.registerCommand("inference.applyTerminalPath", () => {
      applyTerminalPath(context);
    })
  );
  context.subscriptions.push(
    vscode10.commands.registerCommand(
      "inference.copyConfigValue",
      (item) => {
        if (item.copyValue) {
          vscode10.env.clipboard.writeText(item.copyValue);
          vscode10.window.showInformationMessage(
            `Copied: ${item.copyValue}`
          );
        }
      }
    )
  );
  context.subscriptions.push(
    vscode10.commands.registerCommand(
      "inference.revealConfigPath",
      (item) => {
        if (item.copyValue) {
          vscode10.commands.executeCommand(
            "revealFileInOS",
            vscode10.Uri.file(item.copyValue)
          );
        }
      }
    )
  );
  context.subscriptions.push(
    vscode10.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("inference")) {
        configProvider.refresh();
      }
    })
  );
  context.subscriptions.push(
    vscode10.commands.registerCommand("inference.resetPathAcceptance", () => {
      const home = inferenceHome();
      const stateKey = `${PATH_FALLBACK_KEY}:${home}`;
      context.globalState.update(stateKey, void 0);
      vscode10.window.showInformationMessage(
        "Inference: PATH fallback preference has been reset."
      );
    })
  );
  applyTerminalPath(context);
  checkToolchain(context, statusBarItem, configProvider).catch(
    (err) => outputChannel.error(`Toolchain check failed: ${err}`)
  );
}
function deactivate() {
}
function applyTerminalPath(context) {
  const binDir = path6.join(inferenceHome(), "bin");
  const sep = process.platform === "win32" ? ";" : ":";
  const env4 = context.environmentVariableCollection;
  env4.prepend("PATH", binDir + sep);
  env4.description = "Adds the Inference toolchain to PATH";
}
var PATH_FALLBACK_KEY = "inference.acceptedPathFallback";
async function checkToolchain(context, statusBarItem, configProvider) {
  const platform2 = detectPlatform();
  const home = inferenceHome();
  const homeIsDefault = !process.env["INFERENCE_HOME"];
  const distServer = process.env["INFS_DIST_SERVER"];
  outputChannel.info("Inference Activation");
  if (!platform2) {
    outputChannel.warn(
      `Platform:         ${process.platform}-${process.arch} (unsupported)`
    );
    outputChannel.info(
      `INFERENCE_HOME:   ${home} ${homeIsDefault ? "(default)" : "(env)"}`
    );
    outputChannel.info(
      `INFS_DIST_SERVER: ${distServer ?? "(not set, using production)"}`
    );
    updateStatusBar(statusBarItem, null);
    vscode10.commands.executeCommand("setContext", "inference.toolchainInstalled", false);
    vscode10.window.showWarningMessage(
      `Inference: unsupported platform (${process.platform}-${process.arch}).`,
      "Download Page"
    ).then((action) => {
      if (action === "Download Page") {
        vscode10.env.openExternal(
          vscode10.Uri.parse(
            "https://github.com/Inferara/inference/releases"
          )
        );
      }
    });
    return;
  }
  outputChannel.info(`Platform:         ${platform2.id}`);
  outputChannel.info(
    `INFERENCE_HOME:   ${home} ${homeIsDefault ? "(default)" : "(env)"}`
  );
  outputChannel.info(
    `INFS_DIST_SERVER: ${distServer ?? "(not set, using production)"}`
  );
  const detection = detectInfs();
  if (!detection) {
    outputChannel.error("infs binary:      not found");
    outputChannel.error("Toolchain status: errors");
    updateStatusBar(statusBarItem, null);
    vscode10.commands.executeCommand("setContext", "inference.toolchainInstalled", false);
    notifyMissing();
    return;
  }
  outputChannel.info(
    `infs binary:      ${detection.path} (${detection.source})`
  );
  if (!homeIsDefault && detection.source === "path") {
    const stateKey = `${PATH_FALLBACK_KEY}:${home}`;
    const accepted = context.globalState.get(stateKey);
    if (accepted === "*") {
      outputChannel.info(
        `Note: Using PATH binary (notification permanently suppressed for this INFERENCE_HOME).`
      );
    } else if (accepted === detection.path) {
      outputChannel.info(
        `Note: Using PATH binary (previously accepted for this INFERENCE_HOME).`
      );
    } else {
      outputChannel.warn(
        `INFERENCE_HOME is set to ${home} but infs was not found there. Using binary from PATH instead.`
      );
      vscode10.window.showWarningMessage(
        `Inference: infs binary not found in INFERENCE_HOME (${home}). Found via PATH instead.`,
        "Install",
        "Dismiss"
      ).then((action) => {
        if (action === "Install") {
          vscode10.commands.executeCommand("inference.installToolchain");
        } else {
          context.globalState.update(stateKey, "*");
        }
      });
    }
  }
  const versionOk = await checkInfsVersion(detection.path);
  if (!versionOk) {
    outputChannel.error("Toolchain status: errors");
    updateStatusBar(statusBarItem, null);
    vscode10.commands.executeCommand("setContext", "inference.toolchainInstalled", false);
    return;
  }
  vscode10.commands.executeCommand("setContext", "inference.toolchainInstalled", true);
  const doctorResult = await runDoctor(detection.path);
  updateStatusBar(statusBarItem, doctorResult);
  configProvider.refresh(detection, doctorResult);
  const status = doctorResult?.hasErrors ? "errors" : doctorResult?.hasWarnings ? "warnings" : "healthy";
  if (doctorResult?.hasErrors) {
    outputChannel.error(`Toolchain status: ${status}`);
  } else if (doctorResult?.hasWarnings) {
    outputChannel.warn(`Toolchain status: ${status}`);
  } else {
    outputChannel.info(`Toolchain status: ${status}`);
  }
  checkForUpdates(detection.path, outputChannel).catch(
    (err) => outputChannel.error(`Update check failed: ${err}`)
  );
}
async function checkInfsVersion(infsPath) {
  try {
    const result = await exec(infsPath, ["version"]);
    if (result.exitCode !== 0) {
      outputChannel.error(
        `infs version failed (exit ${result.exitCode}): ${result.stderr}`
      );
      return false;
    }
    const match = result.stdout.match(/^infs\s+(\S+)/);
    if (!match) {
      outputChannel.error(
        `Could not parse infs version from: ${result.stdout.trim()}`
      );
      return false;
    }
    const version = match[1];
    outputChannel.info(`infs version: ${version}`);
    if (compareSemver(version, MIN_INFS_VERSION) < 0) {
      outputChannel.warn(
        `infs version ${version} is below minimum ${MIN_INFS_VERSION}.`
      );
      vscode10.window.showWarningMessage(
        `Inference: infs version ${version} is outdated (minimum: ${MIN_INFS_VERSION}). Please update.`,
        "Update"
      ).then((action) => {
        if (action === "Update") {
          vscode10.commands.executeCommand("inference.updateToolchain");
        }
      });
      return false;
    }
    return true;
  } catch (err) {
    outputChannel.error(`Failed to run infs version: ${err}`);
    return false;
  }
}
function notifyMissing() {
  vscode10.window.showInformationMessage(
    "Inference toolchain not found. Would you like to install it?",
    "Install",
    "Download Manually",
    "Configure Path"
  ).then((action) => {
    if (action === "Install") {
      vscode10.commands.executeCommand("inference.installToolchain");
    } else if (action === "Download Manually") {
      vscode10.env.openExternal(
        vscode10.Uri.parse(
          "https://github.com/Inferara/inference/releases"
        )
      );
    } else if (action === "Configure Path") {
      vscode10.commands.executeCommand(
        "workbench.action.openSettings",
        "inference.path"
      );
    }
  });
}
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  activate,
  applyTerminalPath,
  deactivate
});
//# sourceMappingURL=extension.js.map
