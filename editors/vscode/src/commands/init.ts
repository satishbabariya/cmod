import * as vscode from 'vscode';
import { getCmodBinaryPath } from '../utils/cmodBinary';
import { getOrCreateTerminal } from '../utils/terminal';

export function registerInitCommand(context: vscode.ExtensionContext): void {
    context.subscriptions.push(
        vscode.commands.registerCommand('cmod.init', async () => {
            await runInit();
        })
    );
}

async function runInit(): Promise<void> {
    // Pick project type
    const projectType = await vscode.window.showQuickPick(
        [
            { label: 'Module', description: 'Single C++20 module project', value: '' },
            { label: 'Workspace', description: 'Multi-member workspace', value: '--workspace' },
        ],
        {
            placeHolder: 'Select project type',
            title: 'cmod: Initialize Project',
        }
    );

    if (!projectType) {
        return;
    }

    // Get project name
    const projectName = await vscode.window.showInputBox({
        prompt: 'Enter project name',
        placeHolder: 'my-project',
        validateInput: (value) => {
            if (!value || value.trim().length === 0) {
                return 'Project name cannot be empty.';
            }
            if (!/^[a-zA-Z][a-zA-Z0-9_-]*$/.test(value)) {
                return 'Project name must start with a letter and contain only letters, digits, hyphens, or underscores.';
            }
            return undefined;
        },
    });

    if (!projectName) {
        return;
    }

    // Choose directory
    const targetDir = await vscode.window.showOpenDialog({
        canSelectFiles: false,
        canSelectFolders: true,
        canSelectMany: false,
        openLabel: 'Select Parent Directory',
        title: 'Where to create the project',
    });

    if (!targetDir || targetDir.length === 0) {
        return;
    }

    const cmodPath = getCmodBinaryPath();
    const parentPath = targetDir[0].fsPath;

    const terminal = getOrCreateTerminal('cmod init');
    terminal.show();

    const args = ['init', projectName];
    if (projectType.value) {
        args.push(projectType.value);
    }

    terminal.sendText(`cd "${parentPath}" && "${cmodPath}" ${args.join(' ')}`);

    // Offer to open the new project
    const open = await vscode.window.showInformationMessage(
        `Project "${projectName}" initialized. Open it?`,
        'Open in Current Window',
        'Open in New Window',
        'Cancel'
    );

    const projectPath = vscode.Uri.file(`${parentPath}/${projectName}`);
    if (open === 'Open in Current Window') {
        vscode.commands.executeCommand('vscode.openFolder', projectPath, false);
    } else if (open === 'Open in New Window') {
        vscode.commands.executeCommand('vscode.openFolder', projectPath, true);
    }
}
