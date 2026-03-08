import * as vscode from 'vscode';
import { BuildStatusNotification } from '../lsp/customMethods';

export interface BuildModuleStatus {
    name: string;
    status: 'pending' | 'compiling' | 'success' | 'failure';
    durationMs?: number;
    errors?: number;
    warnings?: number;
}

export class BuildStatusTreeProvider implements vscode.TreeDataProvider<BuildStatusItem> {
    private _onDidChangeTreeData = new vscode.EventEmitter<BuildStatusItem | undefined | void>();
    readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

    private modules: BuildModuleStatus[] = [];
    private overallStatus: 'idle' | 'building' | 'success' | 'failure' = 'idle';
    private totalModules = 0;
    private completedModules = 0;
    private totalErrors = 0;
    private totalWarnings = 0;

    updateFromNotification(notification: BuildStatusNotification): void {
        this.modules = notification.modules;
        this.overallStatus = notification.status;
        this.totalModules = notification.totalModules;
        this.completedModules = notification.completedModules;
        this.totalErrors = notification.errors;
        this.totalWarnings = notification.warnings;
        this._onDidChangeTreeData.fire();
    }

    getOverallStatus(): { status: string; completed: number; total: number; errors: number; warnings: number } {
        return {
            status: this.overallStatus,
            completed: this.completedModules,
            total: this.totalModules,
            errors: this.totalErrors,
            warnings: this.totalWarnings,
        };
    }

    getTreeItem(element: BuildStatusItem): vscode.TreeItem {
        return element;
    }

    getChildren(element?: BuildStatusItem): Thenable<BuildStatusItem[]> {
        if (element) {
            return Promise.resolve([]);
        }

        if (this.modules.length === 0) {
            const statusText = this.overallStatus === 'idle'
                ? 'No build in progress'
                : `Build ${this.overallStatus}`;

            return Promise.resolve([
                new BuildStatusItem(statusText, vscode.TreeItemCollapsibleState.None, 'info'),
            ]);
        }

        const items = this.modules.map((mod) => {
            let iconId: string;
            switch (mod.status) {
                case 'compiling':
                    iconId = 'loading~spin';
                    break;
                case 'success':
                    iconId = 'check';
                    break;
                case 'failure':
                    iconId = 'error';
                    break;
                case 'pending':
                default:
                    iconId = 'circle-outline';
                    break;
            }

            let label = mod.name;
            if (mod.durationMs !== undefined) {
                label += ` (${mod.durationMs}ms)`;
            }

            const item = new BuildStatusItem(
                label,
                vscode.TreeItemCollapsibleState.None,
                iconId
            );

            const details: string[] = [];
            if (mod.errors && mod.errors > 0) {
                details.push(`${mod.errors} error(s)`);
            }
            if (mod.warnings && mod.warnings > 0) {
                details.push(`${mod.warnings} warning(s)`);
            }
            if (details.length > 0) {
                item.description = details.join(', ');
            }

            return item;
        });

        // Add summary at top
        const summaryText = `Build: ${this.completedModules}/${this.totalModules} modules`;
        const summaryIcon = this.overallStatus === 'success' ? 'check-all' :
            this.overallStatus === 'failure' ? 'error' :
            this.overallStatus === 'building' ? 'loading~spin' : 'dash';

        const summary = new BuildStatusItem(
            summaryText,
            vscode.TreeItemCollapsibleState.None,
            summaryIcon
        );

        if (this.totalErrors > 0 || this.totalWarnings > 0) {
            const parts: string[] = [];
            if (this.totalErrors > 0) { parts.push(`${this.totalErrors} errors`); }
            if (this.totalWarnings > 0) { parts.push(`${this.totalWarnings} warnings`); }
            summary.description = parts.join(', ');
        }

        return Promise.resolve([summary, ...items]);
    }
}

export class BuildStatusItem extends vscode.TreeItem {
    constructor(
        public readonly label: string,
        public readonly collapsibleState: vscode.TreeItemCollapsibleState,
        iconId: string,
    ) {
        super(label, collapsibleState);
        this.iconPath = new vscode.ThemeIcon(iconId);
    }
}
