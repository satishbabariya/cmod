package com.cmod.intellij.listeners

import com.cmod.intellij.CmodPlugin
import com.cmod.intellij.settings.CmodSettingsState
import com.cmod.intellij.util.CmodProcessUtil
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.editor.Document
import com.intellij.openapi.fileEditor.FileDocumentManager
import com.intellij.openapi.fileEditor.FileDocumentManagerListener
import com.intellij.openapi.project.ProjectManager
import com.intellij.openapi.vfs.VirtualFile

/**
 * Listens for file save events and triggers `cmod fmt` and/or `cmod lint`
 * on C++ files when the corresponding settings are enabled.
 */
class CmodSaveListener : FileDocumentManagerListener {

    companion object {
        private val LOG = Logger.getInstance(CmodSaveListener::class.java)

        private val CPP_EXTENSIONS = setOf(
            "cpp", "cxx", "cc", "c",
            "hpp", "hxx", "hh", "h",
            "cppm", "ixx", "mxx"
        )
    }

    override fun beforeDocumentSaving(document: Document) {
        val settings = CmodSettingsState.getInstance()
        if (!settings.formatOnSave && !settings.lintOnSave) {
            return
        }

        val vFile = FileDocumentManager.getInstance().getFile(document) ?: return
        if (!isCppFile(vFile)) {
            return
        }

        // Find the project that contains this file
        val project = ProjectManager.getInstance().openProjects.firstOrNull { project ->
            val basePath = project.basePath ?: return@firstOrNull false
            vFile.path.startsWith(basePath) && CmodPlugin.isCmodProject(project)
        } ?: return

        val filePath = vFile.path

        // Run format and/or lint in background to avoid blocking the save
        ApplicationManager.getApplication().executeOnPooledThread {
            if (settings.formatOnSave) {
                LOG.info("Running cmod fmt on save: $filePath")
                val result = CmodProcessUtil.runCmodCommand(
                    project,
                    listOf("fmt", filePath),
                    timeoutSeconds = 30
                )
                if (!result.isSuccess) {
                    LOG.warn("cmod fmt failed for $filePath: ${result.stderr}")
                } else {
                    // Refresh the file after formatting
                    ApplicationManager.getApplication().invokeLater {
                        vFile.refresh(true, false)
                    }
                }
            }

            if (settings.lintOnSave) {
                LOG.info("Running cmod lint on save: $filePath")
                val result = CmodProcessUtil.runCmodCommand(
                    project,
                    listOf("lint", filePath),
                    timeoutSeconds = 30
                )
                if (!result.isSuccess) {
                    LOG.warn("cmod lint failed for $filePath: ${result.stderr}")
                }
            }
        }
    }

    private fun isCppFile(file: VirtualFile): Boolean {
        val ext = file.extension?.lowercase() ?: return false
        return ext in CPP_EXTENSIONS
    }
}
