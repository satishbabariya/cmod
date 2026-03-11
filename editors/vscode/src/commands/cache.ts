import * as vscode from 'vscode';
import { queryCacheStatusViaCli, getWorkspaceRoot } from '../lsp/customMethods';

export function registerCacheStatusCommand(context: vscode.ExtensionContext): void {
    context.subscriptions.push(
        vscode.commands.registerCommand('cmod.cacheStatus', async () => {
            await showCacheStatus();
        })
    );
}

async function showCacheStatus(): Promise<void> {
    const workspaceRoot = getWorkspaceRoot();
    if (!workspaceRoot) {
        vscode.window.showWarningMessage('cmod: No workspace folder is open.');
        return;
    }

    try {
        const output = await vscode.window.withProgress(
            {
                location: vscode.ProgressLocation.Notification,
                title: 'cmod: Fetching cache status...',
                cancellable: false,
            },
            async () => {
                return await queryCacheStatusViaCli(workspaceRoot);
            }
        );

        // Show in an information message with the output
        const lines = output.trim().split('\n');
        if (lines.length <= 5) {
            vscode.window.showInformationMessage(`cmod Cache Status:\n${output.trim()}`);
        } else {
            // For longer output, show in an output channel
            const channel = vscode.window.createOutputChannel('cmod Cache');
            channel.clear();
            channel.appendLine(output);
            channel.show();
        }
    } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        vscode.window.showErrorMessage(`cmod: ${message}`);
    }
}
