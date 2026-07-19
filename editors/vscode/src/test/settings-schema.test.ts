import * as assert from 'node:assert';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { describe, it } from 'node:test';

/**
 * Validate the package.json schema for settings, commands, and walkthroughs.
 * Covers QA Section 8 (Settings) and QA Section 7 (Walkthrough structure).
 */

const packageJsonPath = path.resolve(__dirname, '..', '..', 'package.json');
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf-8'));
const contributes = packageJson.contributes;

describe('settings schema (QA Section 8)', () => {
    const properties = contributes.configuration.properties;
    const settingKeys = Object.keys(properties);

    it('has exactly 6 settings', () => {
        assert.strictEqual(settingKeys.length, 6);
    });

    it('contains inference.path, inference.autoInstall, inference.checkForUpdates, inference.lsp.enabled, inference.lsp.path, inference-lsp.trace.server', () => {
        assert.ok(settingKeys.includes('inference.path'));
        assert.ok(settingKeys.includes('inference.autoInstall'));
        assert.ok(settingKeys.includes('inference.checkForUpdates'));
        assert.ok(settingKeys.includes('inference.lsp.enabled'));
        assert.ok(settingKeys.includes('inference.lsp.path'));
        assert.ok(settingKeys.includes('inference-lsp.trace.server'));
    });

    it('inference.path has type=string and default=""', () => {
        const setting = properties['inference.path'];
        assert.strictEqual(setting.type, 'string');
        assert.strictEqual(setting.default, '');
    });

    it('inference.autoInstall has type=boolean and default=true', () => {
        const setting = properties['inference.autoInstall'];
        assert.strictEqual(setting.type, 'boolean');
        assert.strictEqual(setting.default, true);
    });

    it('inference.checkForUpdates has type=boolean and default=true', () => {
        const setting = properties['inference.checkForUpdates'];
        assert.strictEqual(setting.type, 'boolean');
        assert.strictEqual(setting.default, true);
    });

    it('inference.lsp.enabled has type=boolean and default=true', () => {
        const setting = properties['inference.lsp.enabled'];
        assert.strictEqual(setting.type, 'boolean');
        assert.strictEqual(setting.default, true);
    });

    it('inference.lsp.path has type=string, default="", and machine scope', () => {
        const setting = properties['inference.lsp.path'];
        assert.strictEqual(setting.type, 'string');
        assert.strictEqual(setting.default, '');
        assert.strictEqual(setting.scope, 'machine');
    });

    it('inference-lsp.trace.server is the standard protocol-trace knob: string enum off/messages/verbose, default off, window scope', () => {
        const setting = properties['inference-lsp.trace.server'];
        assert.strictEqual(setting.type, 'string');
        assert.deepStrictEqual(setting.enum, ['off', 'messages', 'verbose']);
        assert.strictEqual(setting.default, 'off');
        assert.strictEqual(setting.scope, 'window');
    });

    it('inference-lsp.trace.server matches the LanguageClient id so vscode-languageclient picks it up', () => {
        // vscode-languageclient reads `<clientId>.trace.server`; the client
        // is constructed with id 'inference-lsp' in src/lsp/client.ts.
        const traceKeys = settingKeys.filter((k) => k.endsWith('.trace.server'));
        assert.deepStrictEqual(traceKeys, ['inference-lsp.trace.server']);
    });
});

describe('commands schema (QA Section 8)', () => {
    const commands: Array<{ command: string; title: string }> = contributes.commands;

    it('has exactly 11 commands registered', () => {
        assert.strictEqual(commands.length, 11);
    });

    it('contains expected command IDs', () => {
        const ids = commands.map((c) => c.command);
        assert.ok(ids.includes('inference.installToolchain'));
        assert.ok(ids.includes('inference.installComponent'));
        assert.ok(ids.includes('inference.updateToolchain'));
        assert.ok(ids.includes('inference.selectVersion'));
        assert.ok(ids.includes('inference.runDoctor'));
        assert.ok(ids.includes('inference.restartLsp'));
        assert.ok(ids.includes('inference.showOutput'));
        assert.ok(ids.includes('inference.resetPathAcceptance'));
        assert.ok(ids.includes('inference.refreshConfigView'));
        assert.ok(ids.includes('inference.copyConfigValue'));
        assert.ok(ids.includes('inference.revealConfigPath'));
    });
});

describe('activation events (QA Section 1)', () => {
    const activationEvents: string[] = packageJson.activationEvents;

    it('activates on inference language files and .inf workspaces', () => {
        assert.ok(activationEvents.includes('onLanguage:inference'));
        assert.ok(activationEvents.includes('workspaceContains:**/*.inf'));
        assert.ok(activationEvents.includes('onView:inference.configView'));
    });
});

describe('runtime dependencies', () => {
    it('declares vscode-languageclient 9.x (bundled by esbuild, not external)', () => {
        const dependencies = packageJson.dependencies;
        assert.ok(dependencies);
        assert.match(dependencies['vscode-languageclient'], /^\^9\./);
    });
});

describe('walkthrough schema (QA Section 7)', () => {
    const walkthroughs: Array<{
        id: string;
        steps: Array<{ id: string; title: string; description: string }>;
    }> = contributes.walkthroughs;

    it('has exactly 1 walkthrough', () => {
        assert.strictEqual(walkthroughs.length, 1);
    });

    it('walkthrough ID is inference.gettingStarted', () => {
        assert.strictEqual(walkthroughs[0].id, 'inference.gettingStarted');
    });

    it('walkthrough has exactly 4 steps', () => {
        assert.strictEqual(walkthroughs[0].steps.length, 4);
    });

    it('walkthrough steps have correct IDs', () => {
        const stepIds = walkthroughs[0].steps.map((s) => s.id);
        assert.deepStrictEqual(stepIds, [
            'inference.walkthrough.install',
            'inference.walkthrough.verify',
            'inference.walkthrough.createProject',
            'inference.walkthrough.build',
        ]);
    });

    it('createProject step instructs saving as .inf (language-server features are file-scheme only)', () => {
        const step = walkthroughs[0].steps.find(
            (s) => s.id === 'inference.walkthrough.createProject',
        );
        assert.ok(step);
        assert.match(step.description, /save/i);
        assert.match(step.description, /\.inf/);
        assert.ok(
            step.description.includes(
                'command:workbench.action.files.newUntitledFile',
            ),
        );
    });
});
