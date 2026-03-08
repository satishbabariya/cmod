package com.cmod.intellij.actions

import com.cmod.intellij.CmodPlugin
import com.cmod.intellij.util.CmodProcessUtil
import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.ProgressManager
import com.intellij.openapi.progress.Task

/**
 * Action to run `cmod clean` from the Build menu.
 *
 * Removes build artifacts in a background task and refreshes the project
 * file system view.
 */
class CmodCleanAction : AnAction() {

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return

        ProgressManager.getInstance().run(object : Task.Backgroundable(project, "cmod clean", true) {
            override fun run(indicator: ProgressIndicator) {
                indicator.text = "Running cmod clean..."
                indicator.isIndeterminate = true

                val result = CmodProcessUtil.runCmodCommand(project, listOf("clean"))

                ApplicationManager.getApplication().invokeLater {
                    if (result.isSuccess) {
                        // Refresh VFS so the IDE sees the removed files
                        val basePath = project.basePath
                        if (basePath != null) {
                            val vfs = com.intellij.openapi.vfs.LocalFileSystem.getInstance()
                            vfs.findFileByPath(basePath)?.refresh(true, true)
                        }

                        NotificationGroupManager.getInstance()
                            .getNotificationGroup(CmodPlugin.NOTIFICATION_GROUP_ID)
                            .createNotification("cmod clean", "Build artifacts removed", NotificationType.INFORMATION)
                            .notify(project)
                    } else {
                        NotificationGroupManager.getInstance()
                            .getNotificationGroup(CmodPlugin.NOTIFICATION_GROUP_ID)
                            .createNotification(
                                "cmod clean",
                                "Clean failed: ${result.stderr.take(200)}",
                                NotificationType.ERROR
                            )
                            .notify(project)
                    }
                }
            }
        })
    }

    override fun update(e: AnActionEvent) {
        val project = e.project
        e.presentation.isEnabledAndVisible = project != null && CmodPlugin.isCmodProject(project)
    }
}
