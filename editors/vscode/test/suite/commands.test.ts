import * as assert from 'assert';

suite('Commands Test Suite', () => {
    test('Build command module should export registerBuildCommands', async () => {
        const buildModule = await import('../../src/commands/build');
        assert.ok(
            typeof buildModule.registerBuildCommands === 'function',
            'registerBuildCommands is a function'
        );
    });

    test('Test command module should export registerTestCommand', async () => {
        const testModule = await import('../../src/commands/test');
        assert.ok(
            typeof testModule.registerTestCommand === 'function',
            'registerTestCommand is a function'
        );
    });

    test('Run command module should export registerRunCommand', async () => {
        const runModule = await import('../../src/commands/run');
        assert.ok(
            typeof runModule.registerRunCommand === 'function',
            'registerRunCommand is a function'
        );
    });

    test('Clean command module should export registerCleanCommand', async () => {
        const cleanModule = await import('../../src/commands/clean');
        assert.ok(
            typeof cleanModule.registerCleanCommand === 'function',
            'registerCleanCommand is a function'
        );
    });

    test('Init command module should export registerInitCommand', async () => {
        const initModule = await import('../../src/commands/init');
        assert.ok(
            typeof initModule.registerInitCommand === 'function',
            'registerInitCommand is a function'
        );
    });

    test('Format command module should export registerFormatCommand', async () => {
        const fmtModule = await import('../../src/commands/format');
        assert.ok(
            typeof fmtModule.registerFormatCommand === 'function',
            'registerFormatCommand is a function'
        );
    });

    test('Lint command module should export registerLintCommand', async () => {
        const lintModule = await import('../../src/commands/lint');
        assert.ok(
            typeof lintModule.registerLintCommand === 'function',
            'registerLintCommand is a function'
        );
    });

    test('Cache command module should export registerCacheStatusCommand', async () => {
        const cacheModule = await import('../../src/commands/cache');
        assert.ok(
            typeof cacheModule.registerCacheStatusCommand === 'function',
            'registerCacheStatusCommand is a function'
        );
    });

    test('Explain command module should export registerExplainCommand', async () => {
        const explainModule = await import('../../src/commands/explain');
        assert.ok(
            typeof explainModule.registerExplainCommand === 'function',
            'registerExplainCommand is a function'
        );
    });

    test('Command index module should export registerAllCommands', async () => {
        const indexModule = await import('../../src/commands/index');
        assert.ok(
            typeof indexModule.registerAllCommands === 'function',
            'registerAllCommands is a function'
        );
    });
});
