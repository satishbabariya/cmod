import * as vscode from 'vscode';
import { getCmodBinaryPath } from '../utils/cmodBinary';

export function registerBuildCommands(context: vscode.ExtensionContext): void {
    context.subscriptions.push(
        vscode.commands.registerCommand('cmod.build', () => {
            runBuild(false);
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cmod.buildRelease', () => {
            runBuild(true);
        })
    );
}

function runBuild(release: boolean): void {
    const cmodPath = getCmodBinaryPath();
    const config = vscode.workspace.getConfiguration('cmod');
    const jobs = config.get<number>('build.jobs', 0);

    const args: string[] = ['build'];

    if (release || config.get<string>('build.defaultProfile') === 'release') {
        args.push('--release');
    }

    if (jobs > 0) {
        args.push('--jobs', jobs.toString());
    }

    const taskDefinition: vscode.TaskDefinition = {
        type: 'cmod',
        task: 'build',
    };

    const execution = new vscode.ShellExecution(cmodPath, args);

    const task = new vscode.Task(
        taskDefinition,
        vscode.TaskScope.Workspace,
        release ? 'build (release)' : 'build',
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
