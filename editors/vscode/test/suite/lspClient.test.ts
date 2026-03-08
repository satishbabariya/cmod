import * as assert from 'assert';
import * as vscode from 'vscode';

suite('LSP Client Test Suite', () => {
    test('LSP client module should export CmodLspClient', async () => {
        const lspModule = await import('../../src/lsp/client');
        assert.ok(lspModule.CmodLspClient, 'CmodLspClient class exists');
        assert.ok(
            typeof lspModule.CmodLspClient === 'function',
            'CmodLspClient is a constructor'
        );
    });

    test('Custom methods module should export handlers', async () => {
        const customMethods = await import('../../src/lsp/customMethods');
        assert.ok(
            typeof customMethods.registerCustomMethods === 'function',
            'registerCustomMethods is a function'
        );
        assert.ok(
            typeof customMethods.queryDependenciesViaCli === 'function',
            'queryDependenciesViaCli is a function'
        );
        assert.ok(
            typeof customMethods.queryCacheStatusViaCli === 'function',
            'queryCacheStatusViaCli is a function'
        );
        assert.ok(
            typeof customMethods.getWorkspaceRoot === 'function',
            'getWorkspaceRoot is a function'
        );
    });

    test('LSP enabled setting should default to true', () => {
        const config = vscode.workspace.getConfiguration('cmod');
        const lspEnabled = config.get<boolean>('lsp.enabled', true);
        assert.strictEqual(lspEnabled, true, 'LSP should be enabled by default');
    });
});
