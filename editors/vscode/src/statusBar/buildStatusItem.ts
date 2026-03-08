import * as vscode from 'vscode';
import { BuildStatusNotification } from '../lsp/customMethods';

export class BuildStatusItem implements vscode.Disposable {
    private statusBarItem: vscode.StatusBarItem;

    constructor() {
        this.statusBarItem = vscode.window.createStatusBarItem(
            vscode.StatusBarAlignment.Left,
            100
        );
        this.statusBarItem.command = 'cmod-build-status.focus';
        this.statusBarItem.text = '$(package) cmod: ready';
        this.statusBarItem.tooltip = 'cmod build status - click to view details';
        this.statusBarItem.show();
    }

    updateFromNotification(notification: BuildStatusNotification): void {
        const { totalModules, completedModules, status, errors, warnings } = notification;

        switch (status) {
            case 'idle':
                this.statusBarItem.text = '$(package) cmod: idle';
                this.statusBarItem.backgroundColor = undefined;
                break;
            case 'building':
                this.statusBarItem.text = `$(loading~spin) cmod: building ${completedModules}/${totalModules}`;
                this.statusBarItem.backgroundColor = undefined;
                break;
            case 'success': {
                let text = `$(check) cmod: ${completedModules}/${totalModules} ok`;
                if (warnings > 0) {
                    text += ` (${warnings} warning${warnings !== 1 ? 's' : ''})`;
                }
                this.statusBarItem.text = text;
                this.statusBarItem.backgroundColor = undefined;
                break;
            }
            case 'failure': {
                let text = `$(error) cmod: ${errors} error${errors !== 1 ? 's' : ''}`;
                if (warnings > 0) {
                    text += `, ${warnings} warning${warnings !== 1 ? 's' : ''}`;
                }
                this.statusBarItem.text = text;
                this.statusBarItem.backgroundColor = new vscode.ThemeColor(
                    'statusBarItem.errorBackground'
                );
                break;
            }
        }
    }

    setText(text: string): void {
        this.statusBarItem.text = text;
    }

    dispose(): void {
        this.statusBarItem.dispose();
    }
}
