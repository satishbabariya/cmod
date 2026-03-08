import * as vscode from 'vscode';
import { getCmodBinaryPath } from '../utils/cmodBinary';

export function registerCleanCommand(context: vscode.ExtensionContext): void {
    context.subscriptions.push(
        vscode.commands.registerCommand('cmod.clean', async () => {
            await runClean();
        })
    );
}

async function runClean(): Promise<void> {
    const cmodPath = getCmodBinaryPath();

    const taskDefinition: vscode.TaskDefinition = {
        type: 'cmod',
        task: 'clean',
    };

    const execution = new vscode.ShellExecution(cmodPath, ['clean']);

    const task = new vscode.Task(
        taskDefinition,
        vscode.TaskScope.Workspace,
        'clean',
        'cmod',
        execution
    );

    task.presentationOptions = {
        reveal: vscode.TaskRevealKind.Silent,
        panel: vscode.TaskPanelKind.Shared,
    };

    const taskExecution = await vscode.tasks.executeTask(task);

    // Show notification when complete
    const disposable = vscode.tasks.onDidEndTaskProcess((e) => {
        if (e.execution === taskExecution) {
            if (e.exitCode === 0) {
                vscode.window.showInformationMessage('cmod: Clean completed.');
            } else {
                vscode.window.showErrorMessage(`cmod: Clean failed (exit code ${e.exitCode}).`);
            }
            disposable.dispose();
        }
    });
}
