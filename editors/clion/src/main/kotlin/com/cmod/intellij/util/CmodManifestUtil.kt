package com.cmod.intellij.util

import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import java.io.File

/**
 * Utility for parsing and querying cmod.toml manifest files.
 *
 * Provides a lightweight parser for the TOML subset used by cmod manifests,
 * extracting package metadata, dependencies, and build settings.
 */
object CmodManifestUtil {

    private val LOG = Logger.getInstance(CmodManifestUtil::class.java)

    data class ManifestInfo(
        val name: String = "",
        val version: String = "",
        val edition: String = "",
        val moduleName: String = "",
        val buildType: String = "",
        val dependencies: Map<String, String> = emptyMap(),
        val workspaceMembers: List<String> = emptyList(),
    )

    /**
     * Parses the cmod.toml file from the project root directory.
     */
    fun parseManifest(project: Project): ManifestInfo? {
        val basePath = project.basePath ?: return null
        val manifestFile = File(basePath, "cmod.toml")
        if (!manifestFile.exists()) return null
        return parseManifestFile(manifestFile)
    }

    /**
     * Parses a cmod.toml file from the given virtual file.
     */
    fun parseManifest(file: VirtualFile): ManifestInfo? {
        val ioFile = File(file.path)
        if (!ioFile.exists()) return null
        return parseManifestFile(ioFile)
    }

    /**
     * Parses a cmod.toml file from the given java.io.File.
     */
    fun parseManifestFile(file: File): ManifestInfo? {
        return try {
            val lines = file.readLines()
            parseTomlLines(lines)
        } catch (e: Exception) {
            LOG.warn("Failed to parse cmod.toml: ${e.message}")
            null
        }
    }

    /**
     * Lightweight TOML parser sufficient for cmod.toml.
     * Handles [section] headers and key = "value" pairs.
     */
    private fun parseTomlLines(lines: List<String>): ManifestInfo {
        var name = ""
        var version = ""
        var edition = ""
        var moduleName = ""
        var buildType = ""
        val dependencies = mutableMapOf<String, String>()
        val workspaceMembers = mutableListOf<String>()

        var currentSection = ""

        for (rawLine in lines) {
            val line = rawLine.trim()

            // Skip empty lines and comments
            if (line.isEmpty() || line.startsWith("#")) continue

            // Section header
            if (line.startsWith("[") && line.endsWith("]")) {
                currentSection = line.removeSurrounding("[", "]").trim()
                continue
            }

            // Key-value pair
            val eqIndex = line.indexOf('=')
            if (eqIndex < 0) continue

            val key = line.substring(0, eqIndex).trim()
            val rawValue = line.substring(eqIndex + 1).trim()
            val value = unquoteTomlValue(rawValue)

            when (currentSection) {
                "package" -> when (key) {
                    "name" -> name = value
                    "version" -> version = value
                    "edition" -> edition = value
                }
                "module" -> when (key) {
                    "name" -> moduleName = value
                }
                "build" -> when (key) {
                    "type" -> buildType = value
                }
                "dependencies" -> {
                    dependencies[key] = value
                }
                "workspace" -> when (key) {
                    "members" -> {
                        workspaceMembers.addAll(parseTomlArray(rawValue))
                    }
                }
            }
        }

        return ManifestInfo(
            name = name,
            version = version,
            edition = edition,
            moduleName = moduleName,
            buildType = buildType,
            dependencies = dependencies,
            workspaceMembers = workspaceMembers,
        )
    }

    /**
     * Removes surrounding quotes from a TOML string value.
     */
    private fun unquoteTomlValue(value: String): String {
        return when {
            value.startsWith("\"") && value.endsWith("\"") ->
                value.removeSurrounding("\"")
            value.startsWith("'") && value.endsWith("'") ->
                value.removeSurrounding("'")
            else -> value
        }
    }

    /**
     * Parses a TOML inline array like ["a", "b", "c"].
     */
    private fun parseTomlArray(raw: String): List<String> {
        val trimmed = raw.trim()
        if (!trimmed.startsWith("[") || !trimmed.endsWith("]")) return emptyList()

        val inner = trimmed.removeSurrounding("[", "]")
        return inner.split(",")
            .map { it.trim() }
            .filter { it.isNotEmpty() }
            .map { unquoteTomlValue(it) }
    }

    /**
     * Returns the project name from cmod.toml, or the project directory name as fallback.
     */
    fun getProjectName(project: Project): String {
        val manifest = parseManifest(project)
        if (manifest != null && manifest.name.isNotBlank()) return manifest.name
        return project.name
    }

    /**
     * Returns true if the project is a workspace (has [workspace] with members).
     */
    fun isWorkspace(project: Project): Boolean {
        val manifest = parseManifest(project)
        return manifest != null && manifest.workspaceMembers.isNotEmpty()
    }
}
