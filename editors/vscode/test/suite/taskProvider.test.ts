import * as assert from 'assert';

suite('Task Provider Test Suite', () => {
    test('CmodTaskProvider should export correctly', async () => {
        const taskModule = await import('../../src/tasks/cmodTaskProvider');
        assert.ok(taskModule.CmodTaskProvider, 'CmodTaskProvider class exists');
        assert.ok(
            typeof taskModule.CmodTaskProvider === 'function',
            'CmodTaskProvider is a constructor'
        );
    });

    test('CmodTaskProvider should have correct type', async () => {
        const taskModule = await import('../../src/tasks/cmodTaskProvider');
        assert.strictEqual(
            taskModule.CmodTaskProvider.type,
            'cmod',
            'Task provider type should be "cmod"'
        );
    });

    test('CmodTaskProvider instance should implement TaskProvider interface', async () => {
        const taskModule = await import('../../src/tasks/cmodTaskProvider');
        const provider = new taskModule.CmodTaskProvider();

        assert.ok(
            typeof provider.provideTasks === 'function',
            'provideTasks method exists'
        );
        assert.ok(
            typeof provider.resolveTask === 'function',
            'resolveTask method exists'
        );
    });

    test('CmodTaskProvider should provide default tasks', async () => {
        const taskModule = await import('../../src/tasks/cmodTaskProvider');
        const provider = new taskModule.CmodTaskProvider();

        const tasks = await provider.provideTasks();
        assert.ok(Array.isArray(tasks), 'provideTasks returns an array');
        assert.ok(tasks.length > 0, 'At least one default task is provided');

        // Check that build, test, run, clean tasks are present
        const taskNames = tasks.map((t: { name: string }) => t.name);
        assert.ok(
            taskNames.some((n: string) => n.includes('build')),
            'Build task exists'
        );
        assert.ok(
            taskNames.some((n: string) => n.includes('test')),
            'Test task exists'
        );
        assert.ok(
            taskNames.some((n: string) => n.includes('run')),
            'Run task exists'
        );
        assert.ok(
            taskNames.some((n: string) => n.includes('clean')),
            'Clean task exists'
        );
    });
});
