package com.cmod.intellij.util

import com.cmod.intellij.binary.BinaryManager
import com.cmod.intellij.settings.CmodSettingsState
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.util.SystemInfo
import java.io.File
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

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
 */
object CmodBinaryUtil {

    private val LOG = Logger.getInstance(CmodBinaryUtil::class.java)

    private val BINARY_NAME = if (SystemInfo.isWindows) "cmod.exe" else "cmod"

    /**
     * Returns the cmod binary path, triggering auto-download if necessary.
     * Falls back to "cmod" (relying on PATH resolution) if all else fails.
     */
    fun getCmodBinaryPath(): String {
        val binaryManager = BinaryManager.getInstance()
        return binaryManager.ensureBinary() ?: BINARY_NAME
    }

    /**
     * Checks whether the cmod binary is available and executable.
     */
    fun isCmodAvailable(): Boolean {
        val binaryManager = BinaryManager.getInstance()
        return binaryManager.ensureBinary() != null
    }

    /**
     * Returns the cmod version string by running `cmod --version`.
     */
    fun getCmodVersion(): String? {
        val binary = getCmodBinaryPath()
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
