import * as vscode from 'vscode';

const terminals = new Map<string, vscode.Terminal>();

/**
 * Get or create a named terminal. If a terminal with the given name
 * already exists and is still open, it will be reused.
 */
export function getOrCreateTerminal(name: string): vscode.Terminal {
    const existing = terminals.get(name);
    if (existing) {
        // Check if the terminal is still open by trying to access it
        // VS Code doesn't expose a direct "isAlive" check, but terminals
        // in the active list are still valid.
        const allTerminals = vscode.window.terminals;
        if (allTerminals.includes(existing)) {
            return existing;
        }
        // Terminal was closed, remove from cache
        terminals.delete(name);
    }

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    const cwd = workspaceFolder?.uri.fsPath;

    const terminal = vscode.window.createTerminal({
        name: name,
        cwd: cwd,
    });

    terminals.set(name, terminal);

    // Clean up when terminal is closed
    vscode.window.onDidCloseTerminal((closedTerminal) => {
        for (const [key, value] of terminals.entries()) {
            if (value === closedTerminal) {
                terminals.delete(key);
                break;
            }
        }
    });

    return terminal;
}

/**
 * Dispose all tracked terminals.
 */
export function disposeAllTerminals(): void {
    for (const terminal of terminals.values()) {
        terminal.dispose();
    }
    terminals.clear();
}
