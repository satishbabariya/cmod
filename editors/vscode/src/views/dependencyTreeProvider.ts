import * as vscode from 'vscode';
import * as path from 'path';
import { parseManifestDependencies, ManifestDependency } from '../utils/manifestParser';

export class DependencyTreeProvider implements vscode.TreeDataProvider<DependencyItem> {
    private _onDidChangeTreeData = new vscode.EventEmitter<DependencyItem | undefined | void>();
    readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

    private dependencies: ManifestDependency[] = [];

    constructor() {
        this.loadDependencies();
    }

    refresh(): void {
        this.loadDependencies();
        this._onDidChangeTreeData.fire();
    }

    private loadDependencies(): void {
        const folders = vscode.workspace.workspaceFolders;
        if (!folders || folders.length === 0) {
            this.dependencies = [];
            return;
        }

        const manifestPath = path.join(folders[0].uri.fsPath, 'cmod.toml');
        try {
            this.dependencies = parseManifestDependencies(manifestPath);
        } catch {
            this.dependencies = [];
        }
    }

    getTreeItem(element: DependencyItem): vscode.TreeItem {
        return element;
    }

    getChildren(element?: DependencyItem): Thenable<DependencyItem[]> {
        if (element) {
            // Child items show details
            const details: DependencyItem[] = [];
            if (element.dependency) {
                const dep = element.dependency;
                if (dep.version) {
                    details.push(new DependencyItem(
                        `version: ${dep.version}`,
                        vscode.TreeItemCollapsibleState.None,
                        undefined,
                        'symbol-number'
                    ));
                }
                if (dep.git) {
                    details.push(new DependencyItem(
                        `git: ${dep.git}`,
                        vscode.TreeItemCollapsibleState.None,
                        undefined,
                        'repo'
                    ));
                }
                if (dep.branch) {
                    details.push(new DependencyItem(
                        `branch: ${dep.branch}`,
                        vscode.TreeItemCollapsibleState.None,
                        undefined,
                        'git-branch'
                    ));
                }
                if (dep.path) {
                    details.push(new DependencyItem(
                        `path: ${dep.path}`,
                        vscode.TreeItemCollapsibleState.None,
                        undefined,
                        'folder'
                    ));
                }
            }
            return Promise.resolve(details);
        }

        if (this.dependencies.length === 0) {
            return Promise.resolve([
                new DependencyItem(
                    'No dependencies found',
                    vscode.TreeItemCollapsibleState.None,
                    undefined,
                    'info'
                ),
            ]);
        }

        const items = this.dependencies.map((dep) => {
            const label = dep.version
                ? `${dep.name} @ ${dep.version}`
                : dep.name;

            return new DependencyItem(
                label,
                vscode.TreeItemCollapsibleState.Collapsed,
                dep,
                'package'
            );
        });

        return Promise.resolve(items);
    }
}

export class DependencyItem extends vscode.TreeItem {
    constructor(
        public readonly label: string,
        public readonly collapsibleState: vscode.TreeItemCollapsibleState,
        public readonly dependency?: ManifestDependency,
        iconId?: string,
    ) {
        super(label, collapsibleState);
        if (iconId) {
            this.iconPath = new vscode.ThemeIcon(iconId);
        }
        if (dependency) {
            this.tooltip = dependency.git || dependency.path || dependency.version || dependency.name;
            this.contextValue = 'dependency';
        }
    }
}
