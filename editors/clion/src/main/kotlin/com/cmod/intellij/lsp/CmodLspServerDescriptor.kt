package com.cmod.intellij.lsp

import com.cmod.intellij.CmodPlugin
import com.cmod.intellij.settings.CmodSettingsState
import com.cmod.intellij.util.CmodBinaryUtil
import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.platform.lsp.api.ProjectWideLspServerDescriptor

/**
 * Describes how to start and communicate with the cmod LSP server.
 *
 * The server is launched via `cmod lsp` over stdio. It supports C++20 module
 * files (.cppm, .ixx, .mxx, .cpp) and the cmod.toml manifest.
 */
class CmodLspServerDescriptor(project: Project) : ProjectWideLspServerDescriptor(project, "cmod") {

    companion object {
        private val LOG = Logger.getInstance(CmodLspServerDescriptor::class.java)

        private val SUPPORTED_EXTENSIONS = setOf(
            "cppm", "ixx", "mxx",
            "cpp", "cxx", "cc", "c++",
            "h", "hpp", "hxx", "h++"
        )
    }

    override fun isSupportedFile(file: VirtualFile): Boolean {
        val name = file.name
        if (name == CmodPlugin.MANIFEST_FILENAME) return true

        val ext = file.extension?.lowercase() ?: return false
        return ext in SUPPORTED_EXTENSIONS
    }

    override fun createCommandLine(): GeneralCommandLine {
        val settings = CmodSettingsState.getInstance()
        val cmodPath = if (settings.cmodBinaryPath.isNotBlank()) {
            settings.cmodBinaryPath
        } else {
            CmodBinaryUtil.findCmodBinary() ?: "cmod"
        }

        LOG.info("Starting cmod LSP server: $cmodPath lsp")

        return GeneralCommandLine(cmodPath, "lsp").apply {
            withCharset(Charsets.UTF_8)
            withWorkDirectory(project.basePath)
            withEnvironment("CMOD_LSP", "1")
            withRedirectErrorStream(false)
        }
    }
}
