package com.cmod.intellij.lsp

import com.cmod.intellij.CmodPlugin
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.platform.lsp.api.LspServerSupportProvider
import com.intellij.platform.lsp.api.LspServerSupportProvider.LspServerStarter

/**
 * Registers the cmod LSP server with the IntelliJ Platform LSP framework.
 *
 * The server is started when a supported file (C++ module source or cmod.toml)
 * is opened in a project that contains a cmod.toml manifest.
 */
class CmodLspServerSupportProvider : LspServerSupportProvider {

    companion object {
        private val LOG = Logger.getInstance(CmodLspServerSupportProvider::class.java)
    }

    override fun fileOpened(
        project: Project,
        file: VirtualFile,
        serverStarter: LspServerStarter
    ) {
        if (!CmodPlugin.isCmodProject(project)) {
            return
        }

        if (!isSupportedFile(file)) {
            return
        }

        LOG.info("Starting cmod LSP server for file: ${file.path}")
        serverStarter.ensureServerStarted(CmodLspServerDescriptor(project))
    }

    private fun isSupportedFile(file: VirtualFile): Boolean {
        val extension = file.extension?.lowercase()
        val name = file.name

        return when {
            name == CmodPlugin.MANIFEST_FILENAME -> true
            extension in setOf("cppm", "ixx", "mxx") -> true
            extension in setOf("cpp", "cxx", "cc", "c++") -> true
            extension in setOf("h", "hpp", "hxx", "h++") -> true
            else -> false
        }
    }
}
