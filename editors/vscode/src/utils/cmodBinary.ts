import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import * as cp from 'child_process';

/** Cached binary path set by BinaryManager during activation. */
let cachedBinaryPath: string | undefined;

/**
 * Set the cached cmod binary path. Called by extension activation after
 * BinaryManager resolves/downloads the binary.
 */
export function setCachedBinaryPath(binaryPath: string): void {
    cachedBinaryPath = binaryPath;
}

/**
 * Get the path to the cmod binary.
 *
 * Resolution order:
 * 1. cmod.path setting in VS Code configuration
 * 2. Cached path from BinaryManager (set during activation)
 * 3. "cmod" found on the system PATH
 *
 * Returns "cmod" as a fallback (relying on PATH resolution at runtime).
 */
export function getCmodBinaryPath(): string {
    const config = vscode.workspace.getConfiguration('cmod');
    const configuredPath = config.get<string>('path', '');

    if (configuredPath && configuredPath.trim().length > 0) {
        const resolved = resolveHome(configuredPath.trim());
        if (fs.existsSync(resolved)) {
            return resolved;
        }
        // If the configured path doesn't exist, warn the user but still return it
        // so the error surfaces clearly when commands run.
        vscode.window.showWarningMessage(
            `cmod binary not found at configured path: ${resolved}. Falling back to PATH.`
        );
    }

    // Use cached path from BinaryManager if available
    if (cachedBinaryPath && fs.existsSync(cachedBinaryPath)) {
        return cachedBinaryPath;
    }

    // Try to find cmod on PATH
    const pathBinary = findOnPath('cmod');
    if (pathBinary) {
        return pathBinary;
    }

    // Fallback: assume it's on PATH and let the shell resolve it
    return 'cmod';
}

/**
 * Verify that the cmod binary is accessible and return its version string.
 * Returns undefined if cmod is not found or fails.
 */
export function getCmodVersion(): Promise<string | undefined> {
    return new Promise((resolve) => {
        const cmodPath = getCmodBinaryPath();
        cp.exec(`"${cmodPath}" --version`, { timeout: 5000 }, (error, stdout) => {
            if (error) {
                resolve(undefined);
                return;
            }
            resolve(stdout.trim());
        });
    });
}

function resolveHome(filepath: string): string {
    if (filepath.startsWith('~/') || filepath === '~') {
        const home = process.env.HOME || process.env.USERPROFILE || '';
        return path.join(home, filepath.slice(2));
    }
    return filepath;
}

function findOnPath(binaryName: string): string | undefined {
    const pathEnv = process.env.PATH || '';
    const separator = process.platform === 'win32' ? ';' : ':';
    const extensions = process.platform === 'win32' ? ['.exe', '.cmd', '.bat', ''] : [''];

    const dirs = pathEnv.split(separator);
    for (const dir of dirs) {
        for (const ext of extensions) {
            const fullPath = path.join(dir, binaryName + ext);
            try {
                fs.accessSync(fullPath, fs.constants.X_OK);
                return fullPath;
            } catch {
                // Not found in this directory, continue
            }
        }
    }
    return undefined;
}
