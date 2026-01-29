import * as os from 'os';

export type PlatformId = 'linux-x64' | 'macos-arm64' | 'windows-x64';

export interface PlatformInfo {
    id: PlatformId;
    archiveExtension: string;
    binaryName: string;
}

export const SUPPORTED_PLATFORMS: Record<string, PlatformId> = {
    'linux-x64': 'linux-x64',
    'darwin-arm64': 'macos-arm64',
    'win32-x64': 'windows-x64',
};

/**
 * Detect the platform and return its info, or null if unsupported.
 * When osPlatform/osArch are omitted, uses the current runtime values.
 */
export function detectPlatform(
    osPlatform?: string,
    osArch?: string,
): PlatformInfo | null {
    const key = `${osPlatform ?? os.platform()}-${osArch ?? os.arch()}`;
    const id = SUPPORTED_PLATFORMS[key];
    if (!id) {
        return null;
    }
    return {
        id,
        archiveExtension: id === 'windows-x64' ? '.zip' : '.tar.gz',
        binaryName: id === 'windows-x64' ? 'infs.exe' : 'infs',
    };
}
