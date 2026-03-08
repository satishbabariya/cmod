package com.cmod.intellij.binary

import com.cmod.intellij.settings.CmodSettingsState
import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.application.PathManager
import com.intellij.openapi.components.Service
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.ProgressManager
import com.intellij.openapi.progress.Task
import com.intellij.openapi.util.SystemInfo
import java.io.*
import java.net.HttpURLConnection
import java.net.URI
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import java.security.MessageDigest
import java.util.zip.GZIPInputStream
import java.util.zip.ZipInputStream

/**
 * Manages downloading, verifying, and locating the cmod binary.
 *
 * On first use (or version mismatch), downloads the platform-appropriate
 * binary from GitHub Releases, verifies its SHA-256 checksum, and
 * extracts it to the plugin's data directory.
 */
@Service(Service.Level.APP)
class BinaryManager {

    companion object {
        private val LOG = Logger.getInstance(BinaryManager::class.java)
        private const val GITHUB_REPO = "satishbabariya/cmod"
        private const val EXPECTED_VERSION = "0.1.0"

        fun getInstance(): BinaryManager {
            return ApplicationManager.getApplication().getService(BinaryManager::class.java)
        }
    }

    private val binaryName = if (SystemInfo.isWindows) "cmod.exe" else "cmod"
    private val binDir: Path = Paths.get(PathManager.getPluginDataPath(), "cmod", "bin")

    /**
     * Returns the path to the cmod binary, downloading if necessary.
     *
     * Resolution order:
     * 1. User-configured path in settings
     * 2. Previously downloaded binary in plugin data directory
     * 3. System PATH / common locations
     * 4. Auto-download from GitHub Releases
     */
    fun ensureBinary(): String? {
        // 1. User-configured path
        val settings = CmodSettingsState.getInstance()
        if (settings.cmodBinaryPath.isNotBlank()) {
            val configured = File(settings.cmodBinaryPath)
            if (configured.exists() && configured.canExecute()) {
                LOG.info("Using configured cmod binary: ${configured.absolutePath}")
                return configured.absolutePath
            }
            LOG.warn("Configured cmod path not found: ${settings.cmodBinaryPath}")
        }

        // 2. Previously downloaded binary
        val managedPath = binDir.resolve(binaryName)
        if (Files.exists(managedPath) && Files.isExecutable(managedPath)) {
            val version = getVersion(managedPath.toString())
            if (version != null && version.contains(EXPECTED_VERSION)) {
                LOG.info("Using managed cmod binary: $managedPath ($version)")
                return managedPath.toString()
            }
            LOG.info("Managed binary version mismatch (got $version, want $EXPECTED_VERSION)")
        }

        // 3. System PATH and common locations
        val systemBinary = findOnSystem()
        if (systemBinary != null) {
            val version = getVersion(systemBinary)
            if (version != null && version.contains(EXPECTED_VERSION)) {
                LOG.info("Using system cmod binary: $systemBinary ($version)")
                return systemBinary
            }
            LOG.info("System cmod version mismatch (got $version, want $EXPECTED_VERSION)")
        }

        // 4. Auto-download
        val platformInfo = getPlatformInfo()
        if (platformInfo == null) {
            LOG.warn("Unsupported platform: ${SystemInfo.OS_NAME} / ${SystemInfo.OS_ARCH}")
            notify(
                "Unsupported platform for cmod auto-download. Please install cmod manually.",
                NotificationType.ERROR
            )
            return systemBinary
        }

        return downloadBinary(platformInfo)
    }

    private fun downloadBinary(platformInfo: PlatformInfo): String? {
        var result: String? = null

        ProgressManager.getInstance().run(object : Task.WithResult<String?, Exception>(
            null,
            "Downloading cmod v$EXPECTED_VERSION",
            true
        ) {
            override fun compute(indicator: ProgressIndicator): String? {
                indicator.isIndeterminate = false
                indicator.text = "Downloading cmod v$EXPECTED_VERSION..."

                val version = "v$EXPECTED_VERSION"
                val archiveName = "cmod-$version-${platformInfo.target}.${platformInfo.archiveExt}"
                val archiveUrl = "https://github.com/$GITHUB_REPO/releases/download/$version/$archiveName"
                val checksumsUrl = "https://github.com/$GITHUB_REPO/releases/download/$version/checksums-$version.sha256"

                try {
                    // Download archive
                    LOG.info("Downloading $archiveUrl")
                    val archiveBytes = downloadUrl(archiveUrl, indicator)

                    if (indicator.isCanceled) return null

                    // Download and verify checksum
                    indicator.text = "Verifying checksum..."
                    indicator.fraction = 0.8
                    try {
                        val checksumsBytes = downloadUrl(checksumsUrl, null)
                        val checksumsText = String(checksumsBytes, Charsets.UTF_8)
                        val expectedHash = parseChecksumForFile(checksumsText, archiveName)

                        if (expectedHash != null) {
                            val digest = MessageDigest.getInstance("SHA-256")
                            val actualHash = digest.digest(archiveBytes).joinToString("") { "%02x".format(it) }
                            if (actualHash != expectedHash) {
                                LOG.error("Checksum mismatch: expected=$expectedHash, actual=$actualHash")
                                notify("cmod download failed: checksum mismatch", NotificationType.ERROR)
                                return null
                            }
                            LOG.info("Checksum verified: $actualHash")
                        }
                    } catch (e: Exception) {
                        LOG.warn("Could not verify checksum: ${e.message}")
                    }

                    // Extract binary
                    indicator.text = "Extracting binary..."
                    indicator.fraction = 0.9
                    Files.createDirectories(binDir)

                    val destPath = binDir.resolve(binaryName)
                    if (platformInfo.archiveExt == "tar.gz") {
                        extractTarGz(archiveBytes, destPath.toString(), binaryName)
                    } else {
                        extractZip(archiveBytes, destPath.toString(), binaryName)
                    }

                    // Set executable permission on Unix
                    if (!SystemInfo.isWindows) {
                        destPath.toFile().setExecutable(true, false)
                    }

                    indicator.fraction = 1.0
                    LOG.info("cmod binary installed to $destPath")
                    notify("cmod v$EXPECTED_VERSION downloaded successfully.", NotificationType.INFORMATION)

                    return destPath.toString()
                } catch (e: Exception) {
                    LOG.error("Failed to download cmod binary", e)
                    notify(
                        "Failed to download cmod: ${e.message}. Please install manually.",
                        NotificationType.ERROR
                    )
                    return null
                }
            }
        }.also { result = it.compute(ProgressManager.getInstance().progressIndicator ?: EmptyProgressIndicator()) })

        return result
    }

    private fun downloadUrl(url: String, indicator: ProgressIndicator?): ByteArray {
        var currentUrl = url
        var redirectCount = 0
        while (redirectCount < 10) {
            val connection = URI(currentUrl).toURL().openConnection() as HttpURLConnection
            connection.setRequestProperty("User-Agent", "cmod-clion")
            connection.instanceFollowRedirects = false
            connection.connect()

            val responseCode = connection.responseCode
            if (responseCode in 300..399) {
                currentUrl = connection.getHeaderField("Location")
                    ?: throw IOException("Redirect with no Location header")
                redirectCount++
                connection.disconnect()
                continue
            }

            if (responseCode != 200) {
                connection.disconnect()
                throw IOException("HTTP $responseCode for $currentUrl")
            }

            val totalBytes = connection.contentLengthLong
            val output = ByteArrayOutputStream()
            connection.inputStream.use { input ->
                val buffer = ByteArray(8192)
                var bytesRead: Int
                var totalRead = 0L
                while (input.read(buffer).also { bytesRead = it } != -1) {
                    output.write(buffer, 0, bytesRead)
                    totalRead += bytesRead
                    if (indicator != null && totalBytes > 0) {
                        indicator.fraction = totalRead.toDouble() / totalBytes * 0.8
                    }
                }
            }
            connection.disconnect()
            return output.toByteArray()
        }
        throw IOException("Too many redirects for $url")
    }

    private fun extractTarGz(archiveBytes: ByteArray, destPath: String, binaryName: String) {
        val gzipInput = GZIPInputStream(ByteArrayInputStream(archiveBytes))
        val tarBytes = gzipInput.readBytes()
        gzipInput.close()

        // Simple tar parser: 512-byte headers
        var offset = 0
        while (offset < tarBytes.size - 512) {
            val header = tarBytes.sliceArray(offset until offset + 512)
            if (header.all { it == 0.toByte() }) break

            val fileName = String(header, 0, 100, Charsets.UTF_8).trimEnd('\u0000').trim()
            val sizeStr = String(header, 124, 12, Charsets.UTF_8).trimEnd('\u0000').trim()
            val fileSize = if (sizeStr.isNotEmpty()) sizeStr.toLong(8) else 0L

            offset += 512 // Past header

            if (fileName == binaryName || fileName.endsWith("/$binaryName")) {
                val fileData = tarBytes.sliceArray(offset until offset + fileSize.toInt())
                File(destPath).writeBytes(fileData)
                return
            }

            offset += (Math.ceil(fileSize.toDouble() / 512) * 512).toInt()
        }
        throw IOException("Binary '$binaryName' not found in tar.gz archive")
    }

    private fun extractZip(archiveBytes: ByteArray, destPath: String, binaryName: String) {
        ZipInputStream(ByteArrayInputStream(archiveBytes)).use { zis ->
            var entry = zis.nextEntry
            while (entry != null) {
                val name = entry.name
                if (name == binaryName || name.endsWith("/$binaryName")) {
                    File(destPath).outputStream().use { out ->
                        zis.copyTo(out)
                    }
                    return
                }
                entry = zis.nextEntry
            }
        }
        throw IOException("Binary '$binaryName' not found in zip archive")
    }

    private fun parseChecksumForFile(checksumsText: String, fileName: String): String? {
        for (line in checksumsText.lines()) {
            val trimmed = line.trim()
            if (trimmed.isEmpty()) continue
            val parts = trimmed.split(Regex("\\s+"))
            if (parts.size >= 2) {
                val hash = parts[0]
                val name = parts.last().removePrefix("*")
                if (name == fileName || name.endsWith("/$fileName")) {
                    return hash
                }
            }
        }
        return null
    }

    private fun getVersion(binaryPath: String): String? {
        return try {
            val process = ProcessBuilder(binaryPath, "--version")
                .redirectErrorStream(true)
                .start()
            val output = process.inputStream.bufferedReader().readText().trim()
            val exitCode = process.waitFor()
            if (exitCode == 0) output else null
        } catch (e: Exception) {
            null
        }
    }

    private fun findOnSystem(): String? {
        // Search PATH
        val pathEnv = System.getenv("PATH") ?: ""
        val separator = if (SystemInfo.isWindows) ";" else ":"
        for (dir in pathEnv.split(separator)) {
            val candidate = Paths.get(dir, binaryName)
            if (Files.exists(candidate) && Files.isExecutable(candidate)) {
                return candidate.toAbsolutePath().toString()
            }
        }

        // Common install locations
        val commonPaths = listOf(
            Paths.get(System.getProperty("user.home"), ".cargo", "bin", binaryName),
            Paths.get(System.getProperty("user.home"), ".local", "bin", binaryName),
            Paths.get("/usr", "local", "bin", binaryName),
            Paths.get("/opt", "homebrew", "bin", binaryName),
        )
        for (p in commonPaths) {
            if (Files.exists(p) && Files.isExecutable(p)) {
                return p.toAbsolutePath().toString()
            }
        }
        return null
    }

    private fun notify(message: String, type: NotificationType) {
        ApplicationManager.getApplication().invokeLater {
            NotificationGroupManager.getInstance()
                .getNotificationGroup("cmod.notifications")
                .createNotification(message, type)
                .notify(null)
        }
    }

    data class PlatformInfo(
        val target: String,
        val archiveExt: String,
    )

    private fun getPlatformInfo(): PlatformInfo? {
        val osName = SystemInfo.OS_NAME.lowercase()
        val arch = System.getProperty("os.arch")?.lowercase() ?: return null

        return when {
            SystemInfo.isLinux && (arch == "amd64" || arch == "x86_64") ->
                PlatformInfo("x86_64-unknown-linux-gnu", "tar.gz")
            SystemInfo.isLinux && (arch == "aarch64" || arch == "arm64") ->
                PlatformInfo("aarch64-unknown-linux-gnu", "tar.gz")
            SystemInfo.isMac && (arch == "amd64" || arch == "x86_64") ->
                PlatformInfo("x86_64-apple-darwin", "tar.gz")
            SystemInfo.isMac && (arch == "aarch64" || arch == "arm64") ->
                PlatformInfo("aarch64-apple-darwin", "tar.gz")
            SystemInfo.isWindows && (arch == "amd64" || arch == "x86_64") ->
                PlatformInfo("x86_64-pc-windows-msvc", "zip")
            SystemInfo.isWindows && (arch == "aarch64" || arch == "arm64") ->
                PlatformInfo("aarch64-pc-windows-msvc", "zip")
            else -> null
        }
    }

    /**
     * Minimal progress indicator for non-UI contexts.
     */
    private class EmptyProgressIndicator : ProgressIndicator {
        override fun start() {}
        override fun stop() {}
        override fun isRunning(): Boolean = true
        override fun cancel() {}
        override fun isCanceled(): Boolean = false
        override fun setText(text: String?) {}
        override fun getText(): String = ""
        override fun setText2(text: String?) {}
        override fun getText2(): String = ""
        override fun getFraction(): Double = 0.0
        override fun setFraction(fraction: Double) {}
        override fun pushState() {}
        override fun popState() {}
        override fun isModal(): Boolean = false
        override fun getModalityState(): com.intellij.openapi.application.ModalityState =
            com.intellij.openapi.application.ModalityState.nonModal()
        override fun setModalityState(modalityState: com.intellij.openapi.application.ModalityState) {}
        override fun isIndeterminate(): Boolean = true
        override fun setIndeterminate(indeterminate: Boolean) {}
        override fun checkCanceled() {}
        override fun isPopupWasShown(): Boolean = false
        override fun isShowing(): Boolean = false
    }
}
