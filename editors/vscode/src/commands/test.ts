import * as vscode from 'vscode';
import { getCmodBinaryPath } from '../utils/cmodBinary';

export function registerTestCommand(context: vscode.ExtensionContext): void {
    context.subscriptions.push(
        vscode.commands.registerCommand('cmod.test', () => {
            runTest();
        })
    );
}

function runTest(): void {
    const cmodPath = getCmodBinaryPath();
    const config = vscode.workspace.getConfiguration('cmod');
    const args: string[] = ['test'];

    if (config.get<string>('build.defaultProfile') === 'release') {
        args.push('--release');
    }

    const taskDefinition: vscode.TaskDefinition = {
        type: 'cmod',
        task: 'test',
    };

    const execution = new vscode.ShellExecution(cmodPath, args);

    const task = new vscode.Task(
        taskDefinition,
        vscode.TaskScope.Workspace,
        'test',
        'cmod',
        execution,
        '$cmod'
    );

    task.group = vscode.TaskGroup.Test;
    task.presentationOptions = {
        reveal: vscode.TaskRevealKind.Always,
        panel: vscode.TaskPanelKind.Shared,
        clear: true,
    };

    vscode.tasks.executeTask(task);
}
