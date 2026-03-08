import * as vscode from 'vscode';
import { registerBuildCommands } from './build';
import { registerTestCommand } from './test';
import { registerRunCommand } from './run';
import { registerCleanCommand } from './clean';
import { registerInitCommand } from './init';
import { registerFormatCommand } from './format';
import { registerLintCommand } from './lint';
import { registerCacheStatusCommand } from './cache';
import { registerExplainCommand } from './explain';
import { DependencyTreeProvider } from '../views/dependencyTreeProvider';
import { BuildStatusTreeProvider } from '../views/buildStatusTreeProvider';
import { BuildStatusItem } from '../statusBar/buildStatusItem';
import { showModuleGraph } from '../views/moduleGraphPanel';

export function registerAllCommands(
    context: vscode.ExtensionContext,
    depProvider: DependencyTreeProvider,
    _buildProvider: BuildStatusTreeProvider,
    _statusItem: BuildStatusItem,
): void {
    registerBuildCommands(context);
    registerTestCommand(context);
    registerRunCommand(context);
    registerCleanCommand(context);
    registerInitCommand(context);
    registerFormatCommand(context);
    registerLintCommand(context);
    registerCacheStatusCommand(context);
    registerExplainCommand(context);

    // Show module graph command
    context.subscriptions.push(
        vscode.commands.registerCommand('cmod.showGraph', () => {
            showModuleGraph(context);
        })
    );

    // Show dependencies command (refreshes the tree view)
    context.subscriptions.push(
        vscode.commands.registerCommand('cmod.showDeps', () => {
            depProvider.refresh();
            vscode.commands.executeCommand('cmod-dependencies.focus');
        })
    );
}
