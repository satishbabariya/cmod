package com.cmod.intellij

import com.cmod.intellij.util.CmodBinaryUtil
import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.Disposable
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.intellij.openapi.startup.ProjectActivity
import com.intellij.openapi.vfs.VirtualFile

/**
 * cmod plugin lifecycle manager.
 *
 * Activates when a project containing cmod.toml is opened, checking for the
 * cmod binary and notifying the user if it is missing.
 */
class CmodPlugin : ProjectActivity, Disposable {

    companion object {
        private val LOG = Logger.getInstance(CmodPlugin::class.java)
        const val NOTIFICATION_GROUP_ID = "cmod.notifications"
        const val MANIFEST_FILENAME = "cmod.toml"

        /**
         * Returns true if the given project root contains a cmod.toml file.
         */
        fun isCmodProject(project: Project): Boolean {
            val baseDir = project.basePath ?: return false
            val vfs = com.intellij.openapi.vfs.LocalFileSystem.getInstance()
            return vfs.findFileByPath("$baseDir/$MANIFEST_FILENAME") != null
        }

        /**
         * Finds the cmod.toml manifest file in the project root.
         */
        fun findManifest(project: Project): VirtualFile? {
            val baseDir = project.basePath ?: return null
            val vfs = com.intellij.openapi.vfs.LocalFileSystem.getInstance()
            return vfs.findFileByPath("$baseDir/$MANIFEST_FILENAME")
        }
    }

    override suspend fun execute(project: Project) {
        if (!isCmodProject(project)) {
            LOG.info("Project ${project.name} does not contain cmod.toml, skipping cmod activation")
            return
        }

        LOG.info("cmod project detected: ${project.name}")

        val cmodPath = CmodBinaryUtil.findCmodBinary()
        if (cmodPath == null) {
            NotificationGroupManager.getInstance()
                .getNotificationGroup(NOTIFICATION_GROUP_ID)
                .createNotification(
                    "cmod binary not found",
                    "Install cmod or configure the path in Settings > Tools > cmod",
                    NotificationType.WARNING
                )
                .notify(project)
            LOG.warn("cmod binary not found on PATH")
            return
        }

        LOG.info("cmod binary found at: $cmodPath")

        NotificationGroupManager.getInstance()
            .getNotificationGroup(NOTIFICATION_GROUP_ID)
            .createNotification(
                "cmod",
                "cmod project detected. LSP server and tools are ready.",
                NotificationType.INFORMATION
            )
            .notify(project)
    }

    override fun dispose() {
        LOG.info("cmod plugin disposed")
    }
}
