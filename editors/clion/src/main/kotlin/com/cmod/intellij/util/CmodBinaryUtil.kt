package com.cmod.intellij.util

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
 * Search order:
 * 1. User-configured path in settings
 * 2. PATH environment variable
 * 3. Common installation directories (~/.cargo/bin, ~/.local/bin, /usr/local/bin)
 */
object CmodBinaryUtil {

    private val LOG = Logger.getInstance(CmodBinaryUtil::class.java)

    private val BINARY_NAME = if (SystemInfo.isWindows) "cmod.exe" else "cmod"

    private val COMMON_PATHS = listOf(
        Paths.get(System.getProperty("user.home"), ".cargo", "bin", BINARY_NAME),
        Paths.get(System.getProperty("user.home"), ".local", "bin", BINARY_NAME),
        Paths.get("/usr", "local", "bin", BINARY_NAME),
        Paths.get("/opt", "homebrew", "bin", BINARY_NAME),
    )

    /**
     * Finds the cmod binary, checking user settings first, then PATH, then
     * common installation directories.
     *
     * @return The absolute path to the cmod binary, or null if not found.
     */
    fun findCmodBinary(): String? {
        // Check user-configured path first
        val settings = CmodSettingsState.getInstance()
        if (settings.cmodBinaryPath.isNotBlank()) {
            val configured = File(settings.cmodBinaryPath)
            if (configured.exists() && configured.canExecute()) {
                LOG.info("Using configured cmod binary: ${configured.absolutePath}")
                return configured.absolutePath
            }
            LOG.warn("Configured cmod path does not exist or is not executable: ${settings.cmodBinaryPath}")
        }

        // Search PATH
        val pathBinary = findOnPath()
        if (pathBinary != null) {
            LOG.info("Found cmod on PATH: $pathBinary")
            return pathBinary
        }

        // Search common installation directories
        for (path in COMMON_PATHS) {
            if (Files.exists(path) && Files.isExecutable(path)) {
                LOG.info("Found cmod at common path: $path")
                return path.toAbsolutePath().toString()
            }
        }

        LOG.warn("cmod binary not found")
        return null
    }

    /**
     * Searches the PATH environment variable for the cmod binary.
     */
    private fun findOnPath(): String? {
        val pathEnv = System.getenv("PATH") ?: return null
        val separator = if (SystemInfo.isWindows) ";" else ":"

        for (dir in pathEnv.split(separator)) {
            val candidate = Path.of(dir, BINARY_NAME)
            if (Files.exists(candidate) && Files.isExecutable(candidate)) {
                return candidate.toAbsolutePath().toString()
            }
        }
        return null
    }

    /**
     * Returns the cmod binary path for use in command execution.
     * Falls back to "cmod" (relying on PATH resolution) if not found.
     */
    fun getCmodBinaryPath(): String {
        return findCmodBinary() ?: BINARY_NAME
    }

    /**
     * Checks whether the cmod binary is available and executable.
     */
    fun isCmodAvailable(): Boolean {
        return findCmodBinary() != null
    }

    /**
     * Returns the cmod version string by running `cmod --version`.
     */
    fun getCmodVersion(): String? {
        val binary = findCmodBinary() ?: return null
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
