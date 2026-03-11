import * as fs from 'fs';

/**
 * Represents a dependency parsed from cmod.toml.
 */
export interface ManifestDependency {
    name: string;
    version?: string;
    git?: string;
    branch?: string;
    tag?: string;
    path?: string;
}

/**
 * Parse the [dependencies] section from a cmod.toml file.
 *
 * This is a lightweight parser that handles the common cmod.toml dependency formats:
 *   - Simple version string:  `dep_name = "^1.0"`
 *   - Inline table: `dep_name = { git = "https://...", version = "^1.0" }`
 *   - Git URL = version shorthand: `github.com/user/repo = "^1.0"`
 *
 * This does NOT implement a full TOML parser. It handles the subset
 * needed for the dependency tree view.
 */
export function parseManifestDependencies(manifestPath: string): ManifestDependency[] {
    let content: string;
    try {
        content = fs.readFileSync(manifestPath, 'utf8');
    } catch {
        return [];
    }

    const dependencies: ManifestDependency[] = [];
    const lines = content.split('\n');

    let inDependencies = false;

    for (let i = 0; i < lines.length; i++) {
        const line = lines[i].trim();

        // Skip comments and empty lines
        if (line.startsWith('#') || line.length === 0) {
            continue;
        }

        // Check for section headers
        if (line.startsWith('[')) {
            inDependencies = line === '[dependencies]';
            continue;
        }

        if (!inDependencies) {
            continue;
        }

        // Parse key = value
        const eqIndex = line.indexOf('=');
        if (eqIndex === -1) {
            continue;
        }

        const key = line.substring(0, eqIndex).trim();
        const value = line.substring(eqIndex + 1).trim();

        // Remove surrounding quotes from key if present
        const name = key.replace(/^["']|["']$/g, '');

        if (value.startsWith('{')) {
            // Inline table: { git = "...", version = "...", branch = "..." }
            const dep = parseInlineTable(name, value);
            dependencies.push(dep);
        } else {
            // Simple version string: "^1.0.0"
            const version = value.replace(/^["']|["']$/g, '');

            // Check if the name looks like a git URL
            if (name.includes('/') && name.includes('.')) {
                dependencies.push({
                    name: name.split('/').pop() || name,
                    version: version,
                    git: name.startsWith('http') ? name : `https://${name}`,
                });
            } else {
                dependencies.push({
                    name: name,
                    version: version,
                });
            }
        }
    }

    return dependencies;
}

function parseInlineTable(name: string, value: string): ManifestDependency {
    const dep: ManifestDependency = { name };

    // Remove braces
    const inner = value.replace(/^\{|\}$/g, '').trim();

    // Split by comma, handling quoted values
    const pairs = splitInlineTable(inner);

    for (const pair of pairs) {
        const eqIdx = pair.indexOf('=');
        if (eqIdx === -1) { continue; }

        const k = pair.substring(0, eqIdx).trim();
        const v = pair.substring(eqIdx + 1).trim().replace(/^["']|["']$/g, '');

        switch (k) {
            case 'git':
                dep.git = v;
                // Derive name from git URL if name is a URL-like string
                if (name.includes('/')) {
                    dep.name = name.split('/').pop() || name;
                }
                break;
            case 'version':
                dep.version = v;
                break;
            case 'branch':
                dep.branch = v;
                break;
            case 'tag':
                dep.tag = v;
                break;
            case 'path':
                dep.path = v;
                break;
        }
    }

    return dep;
}

function splitInlineTable(inner: string): string[] {
    const parts: string[] = [];
    let current = '';
    let inQuote = false;
    let quoteChar = '';

    for (let i = 0; i < inner.length; i++) {
        const ch = inner[i];

        if (inQuote) {
            current += ch;
            if (ch === quoteChar) {
                inQuote = false;
            }
        } else if (ch === '"' || ch === "'") {
            inQuote = true;
            quoteChar = ch;
            current += ch;
        } else if (ch === ',') {
            parts.push(current.trim());
            current = '';
        } else {
            current += ch;
        }
    }

    if (current.trim().length > 0) {
        parts.push(current.trim());
    }

    return parts;
}
