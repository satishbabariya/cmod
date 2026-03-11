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
 * Action to run `cmod fmt` from the Build menu.
 *
 * Formats all C++ source files in the project using clang-format via cmod.
 * After formatting, refreshes the VFS so the IDE picks up changes.
 */
class CmodFormatAction : AnAction() {

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return

        ProgressManager.getInstance().run(object : Task.Backgroundable(project, "cmod fmt", true) {
            override fun run(indicator: ProgressIndicator) {
                indicator.text = "Running cmod fmt..."
                indicator.isIndeterminate = true

                val result = CmodProcessUtil.runCmodCommand(project, listOf("fmt"))

                ApplicationManager.getApplication().invokeLater {
                    // Refresh VFS to pick up formatting changes
                    val basePath = project.basePath
                    if (basePath != null) {
                        val vfs = com.intellij.openapi.vfs.LocalFileSystem.getInstance()
                        vfs.findFileByPath(basePath)?.refresh(true, true)
                    }

                    if (result.isSuccess) {
                        NotificationGroupManager.getInstance()
                            .getNotificationGroup(CmodPlugin.NOTIFICATION_GROUP_ID)
                            .createNotification("cmod fmt", "Formatting complete", NotificationType.INFORMATION)
                            .notify(project)
                    } else {
                        NotificationGroupManager.getInstance()
                            .getNotificationGroup(CmodPlugin.NOTIFICATION_GROUP_ID)
                            .createNotification(
                                "cmod fmt",
                                "Formatting failed: ${result.stderr.take(200)}",
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
