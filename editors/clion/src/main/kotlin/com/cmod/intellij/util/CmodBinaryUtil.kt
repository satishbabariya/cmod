package com.cmod.intellij.util

import com.cmod.intellij.binary.BinaryManager
import com.cmod.intellij.settings.CmodSettingsState
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.util.SystemInfo
import java.io.File
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import java.util.concurrent.atomic.AtomicReference

/**
 * Utility for locating the cmod binary on the system.
 *
 * Delegates to [BinaryManager] which handles auto-download when the binary
 * is not found locally.
 *
 * Search order:
 * 1. User-configured path in settings
 * 2. Previously auto-downloaded binary in plugin data directory
 * 3. PATH environment variable
 * 4. Common installation directories (~/.cargo/bin, ~/.local/bin, /usr/local/bin)
 * 5. Auto-download from GitHub Releases (via BinaryManager)
 *
 * ## Threading Notes
 *
 * Methods that call [BinaryManager.ensureBinary] (such as [getCmodBinaryPath],
 * [findCmodBinary], and [isCmodAvailable]) may block with ProgressManager and
 * should NOT be called from the EDT or during read actions. Use the non-blocking
 * variants ([getCmodBinaryPathCached], [isCmodAvailableCached]) for those contexts.
 */
object CmodBinaryUtil {

    private val LOG = Logger.getInstance(CmodBinaryUtil::class.java)

    private val BINARY_NAME = if (SystemInfo.isWindows) "cmod.exe" else "cmod"

    /** Cached resolved binary path after first successful resolution. */
    private val cachedBinaryPath = AtomicReference<String?>(null)

    /**
     * Returns the cmod binary path, triggering auto-download if necessary.
     * Falls back to "cmod" (relying on PATH resolution) if all else fails.
     *
     * **Warning:** This method may block with ProgressManager. Do not call from
     * EDT or read actions. Use [getCmodBinaryPathCached] for non-blocking access.
     */
    fun getCmodBinaryPath(): String {
        val binaryManager = BinaryManager.getInstance()
        val path = binaryManager.ensureBinary() ?: BINARY_NAME
        cachedBinaryPath.set(path)
        return path
    }

    /**
     * Returns the cached cmod binary path if previously resolved, or null.
     * This method never triggers downloads and is safe to call from any thread.
     */
    fun getCmodBinaryPathCached(): String? {
        return cachedBinaryPath.get()
    }

    /**
     * Returns the cmod binary path if available, or null if not found.
     * Triggers auto-download if necessary.
     *
     * **Warning:** This method may block with ProgressManager. Do not call from
     * EDT or read actions.
     */
    fun findCmodBinary(): String? {
        val path = BinaryManager.getInstance().ensureBinary()
        if (path != null) {
            cachedBinaryPath.set(path)
        }
        return path
    }

    /**
     * Checks whether the cmod binary is available and executable.
     *
     * **Warning:** This method may block with ProgressManager. Do not call from
     * EDT or read actions. Use [isCmodAvailableCached] for non-blocking access.
     */
    fun isCmodAvailable(): Boolean {
        val binaryManager = BinaryManager.getInstance()
        val path = binaryManager.ensureBinary()
        if (path != null) {
            cachedBinaryPath.set(path)
        }
        return path != null
    }

    /**
     * Returns whether a cmod binary path has been cached from a previous resolution.
     * This method never triggers downloads and is safe to call from any thread.
     *
     * @param skipAutoDownload If true, only checks the cache. If false and cache
     *        is empty, this still returns false (does not trigger download).
     */
    fun isCmodAvailableCached(skipAutoDownload: Boolean = true): Boolean {
        return cachedBinaryPath.get() != null
    }

    /**
     * Clears the cached binary path. Useful when settings change or binary
     * is manually removed.
     */
    fun clearCache() {
        cachedBinaryPath.set(null)
    }

    /**
     * Returns the cmod version string by running `cmod --version`.
     */
    fun getCmodVersion(): String? {
        val binary = getCmodBinaryPathCached() ?: return null
        return try {
            val process = ProcessBuilder(binary, "--version")
                .redirectErrorStream(true)
                .start()
            val output = process.inputStream.bufferedReader().readText().trim()
            val exitCode = process.waitFor()
            if (exitCode == 0) output else null
        } catch (e: Exception) {
            LOG.warn("Failed to get cmod version", e)
            null
        }
    }
}
