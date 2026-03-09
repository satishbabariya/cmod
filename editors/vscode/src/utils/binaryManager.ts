import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import * as https from 'https';
import * as http from 'http';
import * as crypto from 'crypto';
import * as cp from 'child_process';
import { createGunzip, inflateRawSync } from 'zlib';

/** The cmod version this extension expects. Kept in sync with package.json. */
const EXPECTED_VERSION: string = require('../../package.json').cmod?.binaryVersion ?? require('../../package.json').version;

const GITHUB_REPO = 'satishbabariya/cmod';

interface PlatformInfo {
    target: string;
    binaryName: string;
    archiveExt: string;
}

/**
 * Resolves the current platform to a cmod release target triple.
 */
function getPlatformInfo(): PlatformInfo | undefined {
    const platform = process.platform;
    const arch = process.arch;
    const binaryName = platform === 'win32' ? 'cmod.exe' : 'cmod';

    const mapping: Record<string, Record<string, { target: string; ext: string }>> = {
        linux: {
            x64: { target: 'x86_64-unknown-linux-gnu', ext: 'tar.gz' },
            arm64: { target: 'aarch64-unknown-linux-gnu', ext: 'tar.gz' },
        },
        darwin: {
            x64: { target: 'x86_64-apple-darwin', ext: 'tar.gz' },
            arm64: { target: 'aarch64-apple-darwin', ext: 'tar.gz' },
        },
        win32: {
            x64: { target: 'x86_64-pc-windows-msvc', ext: 'zip' },
            arm64: { target: 'aarch64-pc-windows-msvc', ext: 'zip' },
        },
    };

    // Check for Alpine/musl
    if (platform === 'linux' && arch === 'x64') {
        try {
            const lddOutput = cp.execSync('ldd --version 2>&1 || true', { encoding: 'utf-8' });
            if (lddOutput.includes('musl')) {
                return { target: 'x86_64-unknown-linux-musl', binaryName, archiveExt: 'tar.gz' };
            }
        } catch {
            // Fall through to glibc default
        }
    }

    const entry = mapping[platform]?.[arch];
    if (!entry) {
        return undefined;
    }

    return { target: entry.target, binaryName, archiveExt: entry.ext };
}

/** Default timeout in milliseconds for HTTP requests. */
const REQUEST_TIMEOUT_MS = 30000;
const SOCKET_TIMEOUT_MS = 15000;

/**
 * Downloads a URL, following redirects, and returns the data as a Buffer.
 */
function download(url: string, onProgress?: (percent: number) => void, timeoutMs: number = REQUEST_TIMEOUT_MS): Promise<Buffer> {
    return new Promise((resolve, reject) => {
        const get = url.startsWith('https') ? https.get : http.get;
        const request = get(url, { headers: { 'User-Agent': 'cmod-vscode' } }, (res) => {
            // Follow redirects (pass timeout to recursive call)
            if (res.statusCode && res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
                download(res.headers.location, onProgress, timeoutMs).then(resolve, reject);
                return;
            }

            if (res.statusCode !== 200) {
                reject(new Error(`Download failed: HTTP ${res.statusCode} for ${url}`));
                return;
            }

            const totalBytes = parseInt(res.headers['content-length'] || '0', 10);
            const chunks: Buffer[] = [];
            let receivedBytes = 0;

            res.on('data', (chunk: Buffer) => {
                chunks.push(chunk);
                receivedBytes += chunk.length;
                if (onProgress && totalBytes > 0) {
                    onProgress(Math.round((receivedBytes / totalBytes) * 100));
                }
            });
            res.on('end', () => resolve(Buffer.concat(chunks)));
            res.on('error', reject);
        });

        // Set request timeout
        request.setTimeout(timeoutMs, () => {
            request.destroy();
            reject(new Error(`Download timeout after ${timeoutMs}ms for ${url}`));
        });

        // Set socket timeout when socket is assigned
        request.on('socket', (socket) => {
            socket.setTimeout(SOCKET_TIMEOUT_MS);
            socket.on('timeout', () => {
                request.destroy();
                reject(new Error(`Socket timeout after ${SOCKET_TIMEOUT_MS}ms for ${url}`));
            });
        });

        request.on('error', reject);
    });
}

/**
 * Extracts cmod binary from a tar.gz archive buffer into the target directory.
 */
async function extractTarGz(archiveBuffer: Buffer, destDir: string, binaryName: string): Promise<void> {
    // Simple tar extraction: gunzip then parse tar format
    const gunzipped = await new Promise<Buffer>((resolve, reject) => {
        const gunzip = createGunzip();
        const chunks: Buffer[] = [];
        gunzip.on('data', (chunk: Buffer) => chunks.push(chunk));
        gunzip.on('end', () => resolve(Buffer.concat(chunks)));
        gunzip.on('error', reject);
        gunzip.end(archiveBuffer);
    });

    // Parse tar: find the binary entry
    let offset = 0;
    while (offset < gunzipped.length - 512) {
        const header = gunzipped.subarray(offset, offset + 512);
        // Check for empty block (end of archive)
        if (header.every((b) => b === 0)) {
            break;
        }

        const fileName = header.subarray(0, 100).toString('utf-8').replace(/\0/g, '').trim();
        const sizeOctal = header.subarray(124, 136).toString('utf-8').replace(/\0/g, '').trim();
        const fileSize = parseInt(sizeOctal, 8) || 0;

        offset += 512; // Move past header

        if (fileName === binaryName || fileName.endsWith('/' + binaryName)) {
            const fileData = gunzipped.subarray(offset, offset + fileSize);
            const destPath = path.join(destDir, binaryName);
            fs.writeFileSync(destPath, fileData);
            fs.chmodSync(destPath, 0o755);
            return;
        }

        // Skip to next entry (aligned to 512 bytes)
        offset += Math.ceil(fileSize / 512) * 512;
    }

    throw new Error(`Binary '${binaryName}' not found in archive`);
}

/**
 * Extracts cmod.exe from a zip archive buffer into the target directory.
 *
 * Uses a minimal zip parser — no external dependencies needed.
 */
async function extractZip(archiveBuffer: Buffer, destDir: string, binaryName: string): Promise<void> {
    // Find End of Central Directory record
    let eocdOffset = -1;
    for (let i = archiveBuffer.length - 22; i >= 0; i--) {
        if (archiveBuffer.readUInt32LE(i) === 0x06054b50) {
            eocdOffset = i;
            break;
        }
    }
    if (eocdOffset === -1) {
        throw new Error('Invalid zip archive: EOCD not found');
    }

    const centralDirOffset = archiveBuffer.readUInt32LE(eocdOffset + 16);
    const centralDirEntries = archiveBuffer.readUInt16LE(eocdOffset + 10);

    let cdOffset = centralDirOffset;
    for (let i = 0; i < centralDirEntries; i++) {
        if (archiveBuffer.readUInt32LE(cdOffset) !== 0x02014b50) {
            break;
        }

        const fileNameLen = archiveBuffer.readUInt16LE(cdOffset + 28);
        const extraLen = archiveBuffer.readUInt16LE(cdOffset + 30);
        const commentLen = archiveBuffer.readUInt16LE(cdOffset + 32);
        const localHeaderOffset = archiveBuffer.readUInt32LE(cdOffset + 42);
        const fileName = archiveBuffer.subarray(cdOffset + 46, cdOffset + 46 + fileNameLen).toString('utf-8');

        if (fileName === binaryName || fileName.endsWith('/' + binaryName)) {
            // Read from local file header
            const lfhNameLen = archiveBuffer.readUInt16LE(localHeaderOffset + 26);
            const lfhExtraLen = archiveBuffer.readUInt16LE(localHeaderOffset + 28);
            const compressedSize = archiveBuffer.readUInt32LE(localHeaderOffset + 18);
            const uncompressedSize = archiveBuffer.readUInt32LE(localHeaderOffset + 22);
            const dataOffset = localHeaderOffset + 30 + lfhNameLen + lfhExtraLen;

            const compressionMethod = archiveBuffer.readUInt16LE(localHeaderOffset + 8);
            const compressedData = archiveBuffer.subarray(dataOffset, dataOffset + compressedSize);

            let fileData: Buffer;
            if (compressionMethod === 0) {
                // Stored (no compression)
                fileData = compressedData;
            } else if (compressionMethod === 8) {
                // DEFLATE compression
                try {
                    fileData = inflateRawSync(compressedData);
                    if (fileData.length !== uncompressedSize) {
                        throw new Error(
                            `Decompressed size mismatch: expected ${uncompressedSize}, got ${fileData.length}`
                        );
                    }
                } catch (err) {
                    throw new Error(`Failed to decompress DEFLATE entry: ${err}`);
                }
            } else {
                throw new Error(
                    `Unsupported zip compression method ${compressionMethod}; only stored (0) and DEFLATE (8) are supported`
                );
            }

            const destPath = path.join(destDir, binaryName);
            fs.writeFileSync(destPath, fileData);
            return;
        }

        cdOffset += 46 + fileNameLen + extraLen + commentLen;
    }

    throw new Error(`Binary '${binaryName}' not found in archive`);
}

/**
 * Manages downloading and verifying the cmod binary.
 */
export class BinaryManager {
    private readonly binDir: string;
    private readonly outputChannel: vscode.OutputChannel;

    constructor(context: vscode.ExtensionContext, outputChannel: vscode.OutputChannel) {
        this.binDir = path.join(context.globalStorageUri.fsPath, 'bin');
        this.outputChannel = outputChannel;
    }

    /**
     * Returns the path to the cmod binary, downloading it if necessary.
     *
     * Resolution order:
     * 1. User-configured `cmod.path` setting
     * 2. Bundled binary (platform-specific VSIX)
     * 3. Previously downloaded binary in global storage
     * 4. `cmod` on system PATH
     * 5. Auto-download from GitHub Releases
     */
    async ensureBinary(): Promise<string> {
        // 1. User-configured path
        const config = vscode.workspace.getConfiguration('cmod');
        const configuredPath = config.get<string>('path', '').trim();
        if (configuredPath) {
            const resolved = this.resolveHome(configuredPath);
            if (fs.existsSync(resolved)) {
                this.outputChannel.appendLine(`Using configured cmod binary: ${resolved}`);
                return resolved;
            }
            this.outputChannel.appendLine(`Configured path not found: ${resolved}, trying other sources.`);
        }

        const platformInfo = getPlatformInfo();
        const binaryName = platformInfo?.binaryName ?? (process.platform === 'win32' ? 'cmod.exe' : 'cmod');

        // 2. Bundled binary (shipped inside the VSIX for platform-specific packages)
        const bundledPath = path.join(__dirname, '..', 'bin', binaryName);
        if (fs.existsSync(bundledPath)) {
            this.outputChannel.appendLine(`Using bundled cmod binary: ${bundledPath}`);
            return bundledPath;
        }

        // 3. Previously downloaded binary
        const managedPath = path.join(this.binDir, binaryName);
        if (fs.existsSync(managedPath)) {
            const version = this.getVersion(managedPath);
            if (version && version.includes(EXPECTED_VERSION)) {
                this.outputChannel.appendLine(`Using managed cmod binary: ${managedPath} (${version})`);
                return managedPath;
            }
            this.outputChannel.appendLine(`Managed binary version mismatch (got ${version}, want ${EXPECTED_VERSION}), will re-download.`);
        }

        // 4. System PATH
        const systemPath = this.findOnPath(binaryName);
        if (systemPath) {
            const version = this.getVersion(systemPath);
            if (version && version.includes(EXPECTED_VERSION)) {
                this.outputChannel.appendLine(`Using system cmod binary: ${systemPath} (${version})`);
                return systemPath;
            }
            // System binary exists but wrong version — still try auto-download
            this.outputChannel.appendLine(`System cmod version mismatch (got ${version}, want ${EXPECTED_VERSION}).`);
        }

        // 5. Auto-download
        if (!platformInfo) {
            const msg = `Unsupported platform: ${process.platform}/${process.arch}. Please install cmod manually and set cmod.path.`;
            vscode.window.showErrorMessage(msg);
            // Fall back to system PATH binary or bare name
            return systemPath ?? 'cmod';
        }

        return this.downloadBinary(platformInfo);
    }

    private async downloadBinary(platformInfo: PlatformInfo): Promise<string> {
        const version = `v${EXPECTED_VERSION}`;
        const archiveName = `cmod-${version}-${platformInfo.target}.${platformInfo.archiveExt}`;
        const archiveUrl = `https://github.com/${GITHUB_REPO}/releases/download/${version}/${archiveName}`;
        const checksumsUrl = `https://github.com/${GITHUB_REPO}/releases/download/${version}/checksums-${version}.sha256`;

        const destPath = path.join(this.binDir, platformInfo.binaryName);

        return vscode.window.withProgress(
            {
                location: vscode.ProgressLocation.Notification,
                title: 'cmod',
                cancellable: true,
            },
            async (progress, token) => {
                progress.report({ message: `Downloading cmod ${version}...` });
                this.outputChannel.appendLine(`Downloading ${archiveUrl}`);

                if (token.isCancellationRequested) {
                    throw new Error('Download cancelled');
                }

                // Download archive
                const archiveBuffer = await download(archiveUrl, (percent) => {
                    progress.report({ message: `Downloading cmod ${version}... ${percent}%`, increment: 0 });
                });

                if (token.isCancellationRequested) {
                    throw new Error('Download cancelled');
                }

                // Download and verify checksum
                progress.report({ message: 'Verifying checksum...' });
                try {
                    const checksumsData = await download(checksumsUrl);
                    const checksumsText = checksumsData.toString('utf-8');
                    const expectedHash = this.parseChecksumForFile(checksumsText, archiveName);

                    if (expectedHash) {
                        const actualHash = crypto.createHash('sha256').update(archiveBuffer).digest('hex');
                        if (actualHash !== expectedHash) {
                            throw new Error(
                                `Checksum mismatch for ${archiveName}:\n  expected: ${expectedHash}\n  actual:   ${actualHash}`
                            );
                        }
                        this.outputChannel.appendLine(`Checksum verified: ${actualHash}`);
                    } else {
                        this.outputChannel.appendLine(`Warning: no checksum found for ${archiveName}, skipping verification.`);
                    }
                } catch (err) {
                    if (err instanceof Error && err.message.includes('Checksum mismatch')) {
                        throw err;
                    }
                    this.outputChannel.appendLine(`Warning: could not verify checksum: ${err}`);
                }

                // Extract binary
                progress.report({ message: 'Extracting binary...' });
                fs.mkdirSync(this.binDir, { recursive: true });

                if (platformInfo.archiveExt === 'tar.gz') {
                    await extractTarGz(archiveBuffer, this.binDir, platformInfo.binaryName);
                } else {
                    await extractZip(archiveBuffer, this.binDir, platformInfo.binaryName);
                }

                this.outputChannel.appendLine(`cmod binary installed to ${destPath}`);
                vscode.window.showInformationMessage(`cmod ${version} downloaded successfully.`);
                return destPath;
            }
        );
    }

    private parseChecksumForFile(checksumsText: string, fileName: string): string | undefined {
        for (const line of checksumsText.split('\n')) {
            const trimmed = line.trim();
            if (!trimmed) {
                continue;
            }
            // Format: "hash  filename" or "hash *filename"
            const parts = trimmed.split(/\s+/);
            if (parts.length >= 2) {
                const hash = parts[0];
                const name = parts[parts.length - 1].replace(/^\*/, '');
                if (name === fileName || name.endsWith('/' + fileName)) {
                    return hash;
                }
            }
        }
        return undefined;
    }

    private getVersion(binaryPath: string): string | undefined {
        try {
            const result = cp.execSync(`"${binaryPath}" --version`, {
                timeout: 5000,
                encoding: 'utf-8',
                stdio: ['pipe', 'pipe', 'pipe'],
            });
            return result.trim();
        } catch {
            return undefined;
        }
    }

    private findOnPath(binaryName: string): string | undefined {
        const pathEnv = process.env.PATH || '';
        const separator = process.platform === 'win32' ? ';' : ':';
        const extensions = process.platform === 'win32' ? ['.exe', '.cmd', '.bat', ''] : [''];

        for (const dir of pathEnv.split(separator)) {
            for (const ext of extensions) {
                const fullPath = path.join(dir, binaryName + (ext && !binaryName.endsWith(ext) ? ext : ''));
                try {
                    fs.accessSync(fullPath, fs.constants.X_OK);
                    return fullPath;
                } catch {
                    // continue
                }
            }
        }
        return undefined;
    }

    private resolveHome(filepath: string): string {
        if (filepath.startsWith('~/') || filepath === '~') {
            const home = process.env.HOME || process.env.USERPROFILE || '';
            return path.join(home, filepath.slice(2));
        }
        return filepath;
    }
}
