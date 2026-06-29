import * as vscode from 'vscode';
import * as path from 'path';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext) {
    const config = vscode.workspace.getConfiguration('buildlang');
    const serverPath = config.get<string>('serverPath', 'buildc');

    const serverOptions: ServerOptions = {
        command: serverPath,
        args: ['lsp'],
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'buildlang' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.bld'),
        },
    };

    client = new LanguageClient(
        'buildlang',
        'BuildLang Language Server',
        serverOptions,
        clientOptions
    );

    client.start().catch((err) => {
        // The language server isn't available yet - syntax highlighting
        // still works without it. Log silently so users aren't alarmed.
        console.log(
            'BuildLang language server not found. ' +
            'Syntax highlighting is active. ' +
            'Install buildc and set buildlang.serverPath to exercise the partial LSP server.'
        );
    });
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
