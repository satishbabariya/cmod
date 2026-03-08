import * as vscode from 'vscode';
import { getCmodBinaryPath } from '../utils/cmodBinary';
import { getOrCreateTerminal } from '../utils/terminal';

export function registerRunCommand(context: vscode.ExtensionContext): void {
    context.subscriptions.push(
        vscode.commands.registerCommand('cmod.run', () => {
            runProject();
        })
    );
}

function runProject(): void {
    const cmodPath = getCmodBinaryPath();
    const config = vscode.workspace.getConfiguration('cmod');
    const args: string[] = ['run'];

    if (config.get<string>('build.defaultProfile') === 'release') {
        args.push('--release');
    }

    const terminal = getOrCreateTerminal('cmod run');
    terminal.show();
    terminal.sendText(`"${cmodPath}" ${args.join(' ')}`);
}
