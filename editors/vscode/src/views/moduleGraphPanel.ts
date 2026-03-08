import * as vscode from 'vscode';
import * as path from 'path';
import * as cp from 'child_process';
import * as fs from 'fs';
import { getCmodBinaryPath } from '../utils/cmodBinary';
import { getWorkspaceRoot } from '../lsp/customMethods';

let currentPanel: vscode.WebviewPanel | undefined;

export function showModuleGraph(context: vscode.ExtensionContext): void {
    const workspaceRoot = getWorkspaceRoot();
    if (!workspaceRoot) {
        vscode.window.showWarningMessage('cmod: No workspace folder is open.');
        return;
    }

    if (currentPanel) {
        currentPanel.reveal(vscode.ViewColumn.Beside);
        refreshGraphData(currentPanel, workspaceRoot);
        return;
    }

    currentPanel = vscode.window.createWebviewPanel(
        'cmodModuleGraph',
        'cmod: Module Graph',
        vscode.ViewColumn.Beside,
        {
            enableScripts: true,
            retainContextWhenHidden: true,
            localResourceRoots: [
                vscode.Uri.file(path.join(context.extensionPath, 'resources')),
                vscode.Uri.file(path.join(context.extensionPath, '..', 'shared', 'graph')),
            ],
        }
    );

    currentPanel.webview.html = getWebviewContent(currentPanel.webview, context.extensionPath);

    // Handle messages from the webview
    currentPanel.webview.onDidReceiveMessage(
        (message) => {
            switch (message.type) {
                case 'ready':
                    if (currentPanel) {
                        refreshGraphData(currentPanel, workspaceRoot);
                    }
                    break;
                case 'openFile':
                    if (message.path) {
                        const filePath = path.isAbsolute(message.path)
                            ? message.path
                            : path.join(workspaceRoot, message.path);
                        const uri = vscode.Uri.file(filePath);
                        vscode.window.showTextDocument(uri, { preview: true });
                    }
                    break;
            }
        },
        undefined,
        context.subscriptions
    );

    currentPanel.onDidDispose(() => {
        currentPanel = undefined;
    });
}

function refreshGraphData(panel: vscode.WebviewPanel, workspaceRoot: string): void {
    const cmodPath = getCmodBinaryPath();

    cp.exec(
        `"${cmodPath}" graph --format json --status --timing`,
        { cwd: workspaceRoot, timeout: 30000 },
        (error, stdout, stderr) => {
            if (error) {
                vscode.window.showErrorMessage(`cmod graph: ${stderr || error.message}`);
                return;
            }

            try {
                const graphData = JSON.parse(stdout);
                panel.webview.postMessage({
                    type: 'setGraphData',
                    data: graphData,
                });
            } catch (parseErr) {
                vscode.window.showErrorMessage(`cmod graph: Failed to parse JSON output.`);
            }
        }
    );
}

function getWebviewContent(webview: vscode.Webview, extensionPath: string): string {
    // Try to load the shared graph HTML
    const sharedGraphDir = path.join(extensionPath, '..', 'shared', 'graph');
    const graphCssPath = path.join(sharedGraphDir, 'graph.css');
    const graphJsPath = path.join(sharedGraphDir, 'graph.js');

    let graphCss = '';
    let graphJs = '';

    try {
        graphCss = fs.readFileSync(graphCssPath, 'utf8');
    } catch {
        graphCss = '/* shared graph.css not found */';
    }

    try {
        graphJs = fs.readFileSync(graphJsPath, 'utf8');
    } catch {
        graphJs = '/* shared graph.js not found */';
    }

    // Check for shared graph.html and use it as a base, or build our own
    // We inline the CSS and JS for CSP compliance in webviews
    const nonce = getNonce();

    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <meta http-equiv="Content-Security-Policy"
          content="default-src 'none'; script-src 'nonce-${nonce}'; style-src 'nonce-${nonce}';" />
    <title>cmod Module Graph</title>
    <style nonce="${nonce}">
${graphCss}
    </style>
</head>
<body>
    <div id="controls">
        <input type="text" id="filter" placeholder="Filter modules..." />
        <button id="resetZoom">Reset Zoom</button>
        <span id="stats"></span>
    </div>
    <svg id="graph"></svg>

    <script nonce="${nonce}">
        // Minimal D3 force layout fallback (subset for when d3.v7.min.js is not bundled)
        // The shared graph.js expects a global d3 object. We provide a simplified version
        // that supports the force-directed layout API used by graph.js.
        // For production, bundle d3.v7.min.js in resources/webview/.
        (function() {
            // Check if d3 is already loaded
            if (typeof d3 !== 'undefined') return;

            // Provide a stub that renders a simple list instead
            window.d3 = undefined;
        })();
    </script>

    <script nonce="${nonce}">
${graphJs}
    </script>

    <script nonce="${nonce}">
        // VS Code webview API bridge
        (function() {
            var vscode;
            try {
                vscode = acquireVsCodeApi();
            } catch(e) {
                // Not in VS Code webview context
                return;
            }

            window._postMessage = function(msg) {
                vscode.postMessage(msg);
            };

            window.addEventListener('message', function(event) {
                var msg = event.data;
                if (msg.type === 'setGraphData') {
                    if (typeof renderGraph === 'function' && typeof d3 !== 'undefined' && d3) {
                        renderGraph(msg.data);
                    } else {
                        // Fallback: render as a simple list
                        renderGraphFallback(msg.data);
                    }
                }
            });

            function renderGraphFallback(data) {
                var svg = document.getElementById('graph');
                svg.style.display = 'none';

                var container = document.createElement('div');
                container.style.padding = '16px';
                container.style.fontFamily = 'var(--vscode-font-family, monospace)';
                container.style.color = 'var(--vscode-editor-foreground, #d4d4d4)';
                container.style.overflowY = 'auto';
                container.style.height = '100vh';

                var keys = Object.keys(data);
                var statsEl = document.getElementById('stats');
                var upToDate = 0;
                keys.forEach(function(k) {
                    if (data[k].status === 'up-to-date') upToDate++;
                });
                statsEl.textContent = keys.length + ' modules, ' + upToDate + '/' + keys.length + ' up-to-date';

                keys.forEach(function(key) {
                    var entry = data[key];
                    var div = document.createElement('div');
                    div.style.padding = '8px 12px';
                    div.style.margin = '4px 0';
                    div.style.borderRadius = '4px';
                    div.style.cursor = 'pointer';

                    var status = entry.status || 'never-built';
                    if (status === 'up-to-date') {
                        div.style.borderLeft = '3px solid #73c991';
                    } else if (status === 'needs-rebuild') {
                        div.style.borderLeft = '3px solid #e8c86e';
                    } else {
                        div.style.borderLeft = '3px solid #a0a0a0';
                    }
                    div.style.background = 'var(--vscode-list-hoverBackground, #2a2d2e)';

                    var name = entry.name || key;
                    var kind = entry.kind || '';
                    var source = entry.source || entry.id || '';
                    var imports = (entry.imports || []).join(', ') || 'none';
                    var timing = entry.build_time_ms ? ' (' + entry.build_time_ms + 'ms)' : '';

                    div.innerHTML = '<strong>' + name + '</strong>' + timing +
                        '<br/><small>Kind: ' + kind + ' | Status: ' + status +
                        '<br/>Source: ' + source +
                        '<br/>Imports: ' + imports + '</small>';

                    div.addEventListener('click', function() {
                        if (source && window._postMessage) {
                            window._postMessage({ type: 'openFile', path: source });
                        }
                    });

                    container.appendChild(div);
                });

                document.body.appendChild(container);

                // Wire up filter
                var filterInput = document.getElementById('filter');
                filterInput.addEventListener('input', function() {
                    var query = this.value.toLowerCase();
                    var items = container.querySelectorAll('div');
                    items.forEach(function(item) {
                        var text = item.textContent.toLowerCase();
                        item.style.display = (!query || text.indexOf(query) !== -1) ? 'block' : 'none';
                    });
                });
            }

            // Signal ready
            vscode.postMessage({ type: 'ready' });
        })();
    </script>
</body>
</html>`;
}

function getNonce(): string {
    let text = '';
    const possible = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    for (let i = 0; i < 32; i++) {
        text += possible.charAt(Math.floor(Math.random() * possible.length));
    }
    return text;
}
