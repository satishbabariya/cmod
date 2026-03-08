import * as vscode from 'vscode';
import { getCmodBinaryPath } from '../utils/cmodBinary';

export function registerLintCommand(context: vscode.ExtensionContext): void {
    context.subscriptions.push(
        vscode.commands.registerCommand('cmod.lint', () => {
            runLint();
        })
    );
}

function runLint(): void {
    const cmodPath = getCmodBinaryPath();

    const taskDefinition: vscode.TaskDefinition = {
        type: 'cmod',
        task: 'lint',
    };

    const execution = new vscode.ShellExecution(cmodPath, ['lint']);

    const task = new vscode.Task(
        taskDefinition,
        vscode.TaskScope.Workspace,
        'lint',
        'cmod',
        execution,
        '$cmod'
    );

    task.group = vscode.TaskGroup.Build;
    task.presentationOptions = {
        reveal: vscode.TaskRevealKind.Always,
        panel: vscode.TaskPanelKind.Shared,
        clear: true,
    };

    vscode.tasks.executeTask(task);
}
