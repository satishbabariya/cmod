import * as path from 'path';
import Mocha from 'mocha';
import { globSync } from 'glob';

export function run(): Promise<void> {
    const mocha = new Mocha({
        ui: 'tdd',
        color: true,
        timeout: 10000,
    });

    const testsRoot = path.resolve(__dirname);

    return new Promise((resolve, reject) => {
        const globPattern = '**/**.test.js';
        const matches = globSync(globPattern, { cwd: testsRoot });

        matches.forEach((file: string) => {
            mocha.addFile(path.resolve(testsRoot, file));
        });

        try {
            mocha.run((failures: number) => {
                if (failures > 0) {
                    reject(new Error(`${failures} tests failed.`));
                } else {
                    resolve();
                }
            });
        } catch (err) {
            reject(err);
        }
    });
}
