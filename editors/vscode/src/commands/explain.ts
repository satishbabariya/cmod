import * as vscode from 'vscode';
import * as cp from 'child_process';
import { getCmodBinaryPath } from '../utils/cmodBinary';
import { getWorkspaceRoot } from '../lsp/customMethods';

export function registerExplainCommand(context: vscode.ExtensionContext): void {
    context.subscriptions.push(
        vscode.commands.registerCommand('cmod.explain', async () => {
            await runExplain();
        })
    );
}

async function runExplain(): Promise<void> {
    const workspaceRoot = getWorkspaceRoot();
    if (!workspaceRoot) {
        vscode.window.showWarningMessage('cmod: No workspace folder is open.');
        return;
    }

    const moduleName = await vscode.window.showInputBox({
        prompt: 'Enter the module name to explain',
        placeHolder: 'e.g., my_module',
        validateInput: (value) => {
            if (!value || value.trim().length === 0) {
                return 'Module name cannot be empty.';
            }
            return undefined;
        },
    });

    if (!moduleName) {
        return;
    }

    const cmodPath = getCmodBinaryPath();

    try {
        const output = await vscode.window.withProgress(
            {
                location: vscode.ProgressLocation.Notification,
                title: `cmod: Explaining rebuild for "${moduleName}"...`,
                cancellable: false,
            },
            () => {
                return new Promise<string>((resolve, reject) => {
                    cp.exec(
                        `"${cmodPath}" explain "${moduleName}"`,
                        { cwd: workspaceRoot, timeout: 30000 },
                        (error, stdout, stderr) => {
                            if (error) {
                                reject(new Error(stderr || error.message));
                                return;
                            }
                            resolve(stdout);
                        }
                    );
                });
            }
        );

        const channel = vscode.window.createOutputChannel('cmod Explain');
        channel.clear();
        channel.appendLine(`Rebuild explanation for "${moduleName}":`);
        channel.appendLine('');
        channel.appendLine(output);
        channel.show();
    } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        vscode.window.showErrorMessage(`cmod explain: ${message}`);
    }
}
