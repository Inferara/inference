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
var vscode2 = __toESM(require("vscode"));

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
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) {
    const diff = (pa[i] || 0) - (pb[i] || 0);
    if (diff !== 0) {
      return diff;
    }
  }
  return 0;
}

// src/extension.ts
var MIN_INFS_VERSION = "0.1.0";
var outputChannel = vscode2.window.createOutputChannel("Inference");
function activate(context) {
  context.subscriptions.push(outputChannel);
  context.subscriptions.push(
    vscode2.commands.registerCommand("inference.showOutput", () => {
      outputChannel.show();
    })
  );
  for (const cmd of [
    "inference.installToolchain",
    "inference.updateToolchain",
    "inference.selectVersion",
    "inference.runDoctor"
  ]) {
    context.subscriptions.push(
      vscode2.commands.registerCommand(cmd, () => {
        vscode2.window.showInformationMessage(
          "This command will be available in a future update."
        );
      })
    );
  }
  checkToolchain().catch(
    (err) => outputChannel.appendLine(`Toolchain check failed: ${err}`)
  );
}
function deactivate() {
}
async function checkToolchain() {
  const platform2 = detectPlatform();
  if (!platform2) {
    outputChannel.appendLine(
      `Unsupported platform: ${process.platform}-${process.arch}`
    );
    vscode2.window.showWarningMessage(
      `Inference: unsupported platform (${process.platform}-${process.arch}).`,
      "Download Page"
    ).then((action) => {
      if (action === "Download Page") {
        vscode2.env.openExternal(
          vscode2.Uri.parse(
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
    const settings = getSettings();
    if (settings.autoInstall) {
      notifyMissing();
    }
    return;
  }
  outputChannel.appendLine(`infs found: ${infsPath}`);
  const versionOk = await checkInfsVersion(infsPath);
  if (!versionOk) {
    return;
  }
  outputChannel.appendLine("Toolchain detection complete.");
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
      vscode2.window.showWarningMessage(
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
  vscode2.window.showInformationMessage(
    "Inference toolchain not found. Would you like to install it?",
    "Install",
    "Download Manually",
    "Configure Path"
  ).then((action) => {
    if (action === "Install") {
      vscode2.commands.executeCommand("inference.installToolchain");
    } else if (action === "Download Manually") {
      vscode2.env.openExternal(
        vscode2.Uri.parse(
          "https://github.com/Inferara/inference/releases"
        )
      );
    } else if (action === "Configure Path") {
      vscode2.commands.executeCommand(
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
