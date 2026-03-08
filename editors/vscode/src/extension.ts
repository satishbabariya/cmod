import * as vscode from 'vscode';
import { CmodLspClient } from './lsp/client';
import { registerAllCommands } from './commands/index';
import { DependencyTreeProvider } from './views/dependencyTreeProvider';
import { BuildStatusTreeProvider } from './views/buildStatusTreeProvider';
import { CmodTaskProvider } from './tasks/cmodTaskProvider';
import { BuildStatusItem } from './statusBar/buildStatusItem';
import { BinaryManager } from './utils/binaryManager';

let lspClient: CmodLspClient | undefined;
let dependencyTreeProvider: DependencyTreeProvider;
let buildStatusTreeProvider: BuildStatusTreeProvider;
let buildStatusItem: BuildStatusItem;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
    const outputChannel = vscode.window.createOutputChannel('cmod');
    outputChannel.appendLine('cmod extension activating...');

    // Ensure cmod binary is available (downloads if needed)
    const binaryManager = new BinaryManager(context, outputChannel);
    let cmodPath: string;
    try {
        cmodPath = await binaryManager.ensureBinary();
    } catch (err) {
        outputChannel.appendLine(`Failed to obtain cmod binary: ${err}`);
        vscode.window.showErrorMessage(
            'cmod binary not available. Some features will be unavailable. ' +
            'Install cmod manually or set cmod.path in settings.'
        );
        cmodPath = 'cmod'; // Fallback to PATH resolution
    }
    outputChannel.appendLine(`Using cmod binary: ${cmodPath}`);

    // Create tree view providers
    dependencyTreeProvider = new DependencyTreeProvider();
    buildStatusTreeProvider = new BuildStatusTreeProvider();

    const depTreeView = vscode.window.createTreeView('cmod-dependencies', {
        treeDataProvider: dependencyTreeProvider,
        showCollapseAll: true,
    });
    context.subscriptions.push(depTreeView);

    const buildTreeView = vscode.window.createTreeView('cmod-build-status', {
        treeDataProvider: buildStatusTreeProvider,
    });
    context.subscriptions.push(buildTreeView);

    // Create status bar item
    buildStatusItem = new BuildStatusItem();
    context.subscriptions.push(buildStatusItem);

    // Register task provider
    const taskProvider = vscode.tasks.registerTaskProvider(
        'cmod',
        new CmodTaskProvider()
    );
    context.subscriptions.push(taskProvider);

    // Register all commands
    registerAllCommands(context, dependencyTreeProvider, buildStatusTreeProvider, buildStatusItem);

    // Start LSP client if enabled
    const config = vscode.workspace.getConfiguration('cmod');
    if (config.get<boolean>('lsp.enabled', true)) {
        try {
            lspClient = new CmodLspClient(context, outputChannel, buildStatusTreeProvider, buildStatusItem);
            await lspClient.start();
            outputChannel.appendLine('LSP client started.');
        } catch (err) {
            outputChannel.appendLine(`Failed to start LSP client: ${err}`);
            vscode.window.showWarningMessage(
                'cmod LSP server failed to start. Some features may be unavailable.'
            );
        }
    }

    // Watch cmod.toml for changes
    const tomlWatcher = vscode.workspace.createFileSystemWatcher('**/cmod.toml');
    tomlWatcher.onDidChange(() => {
        dependencyTreeProvider.refresh();
        outputChannel.appendLine('cmod.toml changed, refreshing dependencies.');
    });
    tomlWatcher.onDidCreate(() => {
        dependencyTreeProvider.refresh();
        outputChannel.appendLine('cmod.toml created, refreshing dependencies.');
    });
    tomlWatcher.onDidDelete(() => {
        dependencyTreeProvider.refresh();
        outputChannel.appendLine('cmod.toml deleted, refreshing dependencies.');
    });
    context.subscriptions.push(tomlWatcher);

    // Format on save
    const formatOnSaveDisposable = vscode.workspace.onDidSaveTextDocument(async (document) => {
        const cmodConfig = vscode.workspace.getConfiguration('cmod');

        if (isCppDocument(document)) {
            if (cmodConfig.get<boolean>('format.onSave', false)) {
                await vscode.commands.executeCommand('cmod.format');
            }
            if (cmodConfig.get<boolean>('lint.onSave', false)) {
                await vscode.commands.executeCommand('cmod.lint');
            }
        }
    });
    context.subscriptions.push(formatOnSaveDisposable);

    outputChannel.appendLine('cmod extension activated.');
}

export async function deactivate(): Promise<void> {
    if (lspClient) {
        await lspClient.stop();
        lspClient = undefined;
    }
}

function isCppDocument(document: vscode.TextDocument): boolean {
    const cppExtensions = ['.cpp', '.cxx', '.cc', '.c', '.h', '.hpp', '.hxx', '.cppm', '.ixx', '.mxx'];
    return document.languageId === 'cpp' ||
        document.languageId === 'c' ||
        cppExtensions.some(ext => document.fileName.endsWith(ext));
}
