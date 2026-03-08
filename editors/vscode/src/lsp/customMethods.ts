import * as vscode from 'vscode';
import { LanguageClient } from 'vscode-languageclient/node';
import { BuildStatusTreeProvider, BuildModuleStatus } from '../views/buildStatusTreeProvider';
import { BuildStatusItem } from '../statusBar/buildStatusItem';
import { getCmodBinaryPath } from '../utils/cmodBinary';
import * as cp from 'child_process';

/**
 * Notification payload for cmod/buildStatus.
 */
export interface BuildStatusNotification {
    modules: BuildModuleStatus[];
    totalModules: number;
    completedModules: number;
    status: 'idle' | 'building' | 'success' | 'failure';
    errors: number;
    warnings: number;
}

/**
 * Register custom LSP method handlers on the given client.
 */
export function registerCustomMethods(
    client: LanguageClient,
    buildStatusProvider: BuildStatusTreeProvider,
    buildStatusItem: BuildStatusItem,
): void {
    // Handle cmod/buildStatus notifications from the LSP server
    client.onNotification('cmod/buildStatus', (params: BuildStatusNotification) => {
        buildStatusProvider.updateFromNotification(params);
        buildStatusItem.updateFromNotification(params);
    });
}

/**
 * Query dependencies by running `cmod deps --tree` as a CLI fallback.
 * Returns the raw stdout text.
 */
export function queryDependenciesViaCli(workspaceRoot: string): Promise<string> {
    return new Promise((resolve, reject) => {
        const cmodPath = getCmodBinaryPath();
        cp.exec(
            `"${cmodPath}" deps --tree`,
            { cwd: workspaceRoot, timeout: 30000 },
            (error, stdout, stderr) => {
                if (error) {
                    reject(new Error(`cmod deps failed: ${stderr || error.message}`));
                    return;
                }
                resolve(stdout);
            }
        );
    });
}

/**
 * Query cache status by running `cmod cache status` as a CLI fallback.
 * Returns the raw stdout text.
 */
export function queryCacheStatusViaCli(workspaceRoot: string): Promise<string> {
    return new Promise((resolve, reject) => {
        const cmodPath = getCmodBinaryPath();
        cp.exec(
            `"${cmodPath}" cache status`,
            { cwd: workspaceRoot, timeout: 15000 },
            (error, stdout, stderr) => {
                if (error) {
                    reject(new Error(`cmod cache status failed: ${stderr || error.message}`));
                    return;
                }
                resolve(stdout);
            }
        );
    });
}

/**
 * Get the workspace root folder path, or undefined if none is open.
 */
export function getWorkspaceRoot(): string | undefined {
    const folders = vscode.workspace.workspaceFolders;
    if (folders && folders.length > 0) {
        return folders[0].uri.fsPath;
    }
    return undefined;
}
