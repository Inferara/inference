import * as vscode from 'vscode';
import { detectInfs, InfsDetection } from '../toolchain/detection';
import { inferenceHome } from '../toolchain/home';
import { detectPlatform } from '../toolchain/platform';
import { getSettings } from '../config/settings';
import { exec } from '../utils/exec';
import { DoctorResult } from '../toolchain/doctor';

type GroupId = 'toolchain' | 'settings';

export class ConfigItem extends vscode.TreeItem {
    constructor(
        label: string,
        public readonly kind: 'group' | 'property',
        collapsible: vscode.TreeItemCollapsibleState,
        public readonly groupId?: GroupId,
        public readonly settingKey?: string,
        public readonly copyValue?: string,
    ) {
        super(label, collapsible);

        if (kind === 'group') {
            this.iconPath = new vscode.ThemeIcon(
                groupId === 'toolchain' ? 'tools' : 'gear',
            );
        }

        if (settingKey) {
            this.command = {
                title: 'Open Setting',
                command: 'workbench.action.openSettings',
                arguments: [settingKey],
            };
        }

        if (copyValue) {
            this.contextValue = 'inference.configPath';
        }
    }
}

export class InferenceConfigProvider
    implements vscode.TreeDataProvider<ConfigItem>
{
    private _onDidChangeTreeData = new vscode.EventEmitter<
        ConfigItem | undefined | null
    >();
    readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

    private detection: InfsDetection | null = null;
    private version: string | null = null;
    private doctorResult: DoctorResult | null = null;

    refresh(detection?: InfsDetection | null, doctorResult?: DoctorResult | null): void {
        if (detection !== undefined) {
            this.detection = detection;
        }
        if (doctorResult !== undefined) {
            this.doctorResult = doctorResult;
        }
        this._onDidChangeTreeData.fire(undefined);
    }

    getTreeItem(element: ConfigItem): vscode.TreeItem {
        return element;
    }

    async getChildren(element?: ConfigItem): Promise<ConfigItem[]> {
        if (!element) {
            return [
                new ConfigItem(
                    'Toolchain',
                    'group',
                    vscode.TreeItemCollapsibleState.Expanded,
                    'toolchain',
                ),
                new ConfigItem(
                    'Settings',
                    'group',
                    vscode.TreeItemCollapsibleState.Expanded,
                    'settings',
                ),
            ];
        }

        if (element.groupId === 'toolchain') {
            return this.getToolchainChildren();
        }

        if (element.groupId === 'settings') {
            return this.getSettingsChildren();
        }

        return [];
    }

    private async getToolchainChildren(): Promise<ConfigItem[]> {
        const detection = this.detection ?? detectInfs();
        const items: ConfigItem[] = [];

        if (!detection) {
            const item = new ConfigItem(
                'infs: not found',
                'property',
                vscode.TreeItemCollapsibleState.None,
            );
            item.iconPath = new vscode.ThemeIcon('error');
            item.command = {
                title: 'Install Toolchain',
                command: 'inference.installToolchain',
                arguments: [],
            };
            items.push(item);
            return items;
        }

        const infsItem = new ConfigItem(
            `infs: ${detection.path}  (${detection.source})`,
            'property',
            vscode.TreeItemCollapsibleState.None,
            undefined,
            undefined,
            detection.path,
        );
        infsItem.iconPath = new vscode.ThemeIcon('file-binary');
        items.push(infsItem);

        const version = await this.resolveVersion(detection.path);
        const versionItem = new ConfigItem(
            `Version: ${version ?? 'unknown'}`,
            'property',
            vscode.TreeItemCollapsibleState.None,
        );
        versionItem.iconPath = new vscode.ThemeIcon('tag');
        items.push(versionItem);

        const home = inferenceHome();
        const homeIsDefault = !process.env['INFERENCE_HOME'];
        const homeItem = new ConfigItem(
            `Home: ${home}  (${homeIsDefault ? 'default' : 'env'})`,
            'property',
            vscode.TreeItemCollapsibleState.None,
            undefined,
            undefined,
            home,
        );
        homeItem.iconPath = new vscode.ThemeIcon('home');
        items.push(homeItem);

        const platform = detectPlatform();
        const platformItem = new ConfigItem(
            `Platform: ${platform?.id ?? 'unknown'}`,
            'property',
            vscode.TreeItemCollapsibleState.None,
        );
        platformItem.iconPath = new vscode.ThemeIcon('device-desktop');
        items.push(platformItem);

        const status = this.doctorResult
            ? this.doctorResult.hasErrors
                ? 'errors'
                : this.doctorResult.hasWarnings
                    ? 'warnings'
                    : 'healthy'
            : 'unknown';
        const statusIcon = this.doctorResult
            ? this.doctorResult.hasErrors
                ? 'error'
                : this.doctorResult.hasWarnings
                    ? 'warning'
                    : 'pass'
            : 'question';
        const statusItem = new ConfigItem(
            `Status: ${status}`,
            'property',
            vscode.TreeItemCollapsibleState.None,
        );
        statusItem.iconPath = new vscode.ThemeIcon(statusIcon);
        statusItem.command = {
            title: 'Run Doctor',
            command: 'inference.runDoctor',
            arguments: [],
        };
        items.push(statusItem);

        return items;
    }

    private getSettingsChildren(): ConfigItem[] {
        const settings = getSettings();

        const pathItem = new ConfigItem(
            `Path: ${settings.path || '(auto-detect)'}`,
            'property',
            vscode.TreeItemCollapsibleState.None,
            undefined,
            'inference.path',
        );
        pathItem.iconPath = new vscode.ThemeIcon('file-symlink-directory');

        const autoInstallItem = new ConfigItem(
            `Auto Install: ${settings.autoInstall ? 'enabled' : 'disabled'}`,
            'property',
            vscode.TreeItemCollapsibleState.None,
            undefined,
            'inference.autoInstall',
        );
        autoInstallItem.iconPath = new vscode.ThemeIcon('cloud-download');

        const updateItem = new ConfigItem(
            `Check for Updates: ${settings.checkForUpdates ? 'enabled' : 'disabled'}`,
            'property',
            vscode.TreeItemCollapsibleState.None,
            undefined,
            'inference.checkForUpdates',
        );
        updateItem.iconPath = new vscode.ThemeIcon('sync');

        return [pathItem, autoInstallItem, updateItem];
    }

    private async resolveVersion(infsPath: string): Promise<string | null> {
        if (this.version) {
            return this.version;
        }
        try {
            const result = await exec(infsPath, ['version']);
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

    dispose(): void {
        this._onDidChangeTreeData.dispose();
    }
}
