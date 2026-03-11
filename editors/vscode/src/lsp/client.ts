import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
} from 'vscode-languageclient/node';
import { getCmodBinaryPath } from '../utils/cmodBinary';
import { BuildStatusTreeProvider } from '../views/buildStatusTreeProvider';
import { BuildStatusItem } from '../statusBar/buildStatusItem';
import { registerCustomMethods } from './customMethods';

export class CmodLspClient {
    private client: LanguageClient | undefined;
    private context: vscode.ExtensionContext;
    private outputChannel: vscode.OutputChannel;
    private buildStatusProvider: BuildStatusTreeProvider;
    private buildStatusItem: BuildStatusItem;
    private restartCount = 0;
    private readonly maxRestarts = 5;

    constructor(
        context: vscode.ExtensionContext,
        outputChannel: vscode.OutputChannel,
        buildStatusProvider: BuildStatusTreeProvider,
        buildStatusItem: BuildStatusItem,
    ) {
        this.context = context;
        this.outputChannel = outputChannel;
        this.buildStatusProvider = buildStatusProvider;
        this.buildStatusItem = buildStatusItem;
    }

    async start(): Promise<void> {
        const cmodPath = getCmodBinaryPath();

        const serverOptions: ServerOptions = {
            command: cmodPath,
            args: ['lsp'],
        };

        const clientOptions: LanguageClientOptions = {
            documentSelector: [
                { scheme: 'file', language: 'cpp' },
                { scheme: 'file', language: 'c' },
                { scheme: 'file', pattern: '**/cmod.toml' },
                { scheme: 'file', pattern: '**/*.cppm' },
                { scheme: 'file', pattern: '**/*.ixx' },
                { scheme: 'file', pattern: '**/*.mxx' },
            ],
            outputChannel: this.outputChannel,
            traceOutputChannel: this.outputChannel,
            diagnosticCollectionName: 'cmod',
            middleware: {
                handleDiagnostics: (uri, diagnostics, next) => {
                    next(uri, diagnostics);
                },
            },
            initializationFailedHandler: (error) => {
                this.outputChannel.appendLine(`LSP initialization failed: ${error}`);
                return false;
            },
            errorHandler: {
                error: (error, message, count) => {
                    this.outputChannel.appendLine(
                        `LSP error (${count}): ${error?.message ?? message}`
                    );
                    return { action: 1 /* Continue */ };
                },
                closed: () => {
                    this.restartCount++;
                    if (this.restartCount <= this.maxRestarts) {
                        this.outputChannel.appendLine(
                            `LSP server closed. Restarting (${this.restartCount}/${this.maxRestarts})...`
                        );
                        return { action: 1 /* Restart */ };
                    }
                    this.outputChannel.appendLine(
                        `LSP server crashed ${this.maxRestarts} times. Not restarting.`
                    );
                    vscode.window.showErrorMessage(
                        'cmod LSP server has crashed repeatedly. Please check the output for details.'
                    );
                    return { action: 2 /* DoNotRestart */ };
                },
            },
        };

        this.client = new LanguageClient(
            'cmod',
            'cmod Language Server',
            serverOptions,
            clientOptions
        );

        await this.client.start();
        this.restartCount = 0;

        // Register custom LSP method handlers
        registerCustomMethods(this.client, this.buildStatusProvider, this.buildStatusItem);
    }

    async stop(): Promise<void> {
        if (this.client) {
            await this.client.stop();
            this.client = undefined;
        }
    }

    getClient(): LanguageClient | undefined {
        return this.client;
    }

    isRunning(): boolean {
        return this.client !== undefined && this.client.isRunning();
    }
}
