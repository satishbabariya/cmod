import * as vscode from 'vscode';
import { getCmodBinaryPath } from '../utils/cmodBinary';

export function registerFormatCommand(context: vscode.ExtensionContext): void {
    context.subscriptions.push(
        vscode.commands.registerCommand('cmod.format', () => {
            runFormat();
        })
    );
}

function runFormat(): void {
    const cmodPath = getCmodBinaryPath();

    const taskDefinition: vscode.TaskDefinition = {
        type: 'cmod',
        task: 'format',
    };

    const execution = new vscode.ShellExecution(cmodPath, ['fmt']);

    const task = new vscode.Task(
        taskDefinition,
        vscode.TaskScope.Workspace,
        'format',
        'cmod',
        execution,
        '$cmod'
    );

    task.presentationOptions = {
        reveal: vscode.TaskRevealKind.Silent,
        panel: vscode.TaskPanelKind.Shared,
    };

    vscode.tasks.executeTask(task);
}
