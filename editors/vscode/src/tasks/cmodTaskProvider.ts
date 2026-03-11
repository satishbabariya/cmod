import * as vscode from 'vscode';
import { getCmodBinaryPath } from '../utils/cmodBinary';
import { getToolchainEnv } from '../utils/terminal';

interface CmodTaskDefinition extends vscode.TaskDefinition {
    task: string;
    profile?: string;
    args?: string[];
}

export class CmodTaskProvider implements vscode.TaskProvider {
    static readonly type = 'cmod';

    provideTasks(): Thenable<vscode.Task[]> {
        return Promise.resolve(this.getDefaultTasks());
    }

    resolveTask(task: vscode.Task): vscode.Task | undefined {
        const definition = task.definition as CmodTaskDefinition;
        if (definition.task) {
            return this.createTask(definition);
        }
        return undefined;
    }

    private getDefaultTasks(): vscode.Task[] {
        return [
            this.createTask({ type: 'cmod', task: 'build' }),
            this.createTask({ type: 'cmod', task: 'build', profile: 'release' }),
            this.createTask({ type: 'cmod', task: 'test' }),
            this.createTask({ type: 'cmod', task: 'run' }),
            this.createTask({ type: 'cmod', task: 'clean' }),
        ];
    }

    private createTask(definition: CmodTaskDefinition): vscode.Task {
        const cmodPath = getCmodBinaryPath();
        const args: string[] = [definition.task];

        if (definition.profile === 'release') {
            args.push('--release');
        }

        if (definition.args) {
            args.push(...definition.args);
        }

        // Determine task name
        let taskName = definition.task;
        if (definition.profile) {
            taskName += ` (${definition.profile})`;
        }

        // Include toolchain environment variables
        const toolchainEnv = getToolchainEnv();
        const execution = new vscode.ShellExecution(cmodPath, args, {
            env: Object.keys(toolchainEnv).length > 0 ? toolchainEnv : undefined,
        });

        const task = new vscode.Task(
            definition,
            vscode.TaskScope.Workspace,
            taskName,
            'cmod',
            execution,
            '$cmod'
        );

        // Set appropriate task group
        switch (definition.task) {
            case 'build':
                task.group = vscode.TaskGroup.Build;
                break;
            case 'test':
                task.group = vscode.TaskGroup.Test;
                break;
            case 'clean':
                task.group = vscode.TaskGroup.Clean;
                break;
        }

        task.presentationOptions = {
            reveal: vscode.TaskRevealKind.Always,
            panel: vscode.TaskPanelKind.Shared,
            clear: true,
        };

        return task;
    }
}
