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
 * Action to run `cmod build` from the Build menu.
 *
 * Executes the build in a background task with progress indication and
 * reports success or failure via balloon notifications.
 */
class CmodBuildAction : AnAction() {

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return

        ProgressManager.getInstance().run(object : Task.Backgroundable(project, "cmod build", true) {
            override fun run(indicator: ProgressIndicator) {
                indicator.text = "Running cmod build..."
                indicator.isIndeterminate = true

                val result = CmodProcessUtil.runCmodCommand(project, listOf("build"))

                ApplicationManager.getApplication().invokeLater {
                    if (result.isSuccess) {
                        NotificationGroupManager.getInstance()
                            .getNotificationGroup(CmodPlugin.NOTIFICATION_GROUP_ID)
                            .createNotification("cmod build", "Build succeeded", NotificationType.INFORMATION)
                            .notify(project)
                    } else {
                        val diagnostics = CmodProcessUtil.parseClangDiagnostics(result.stderr + result.stdout)
                        val errorCount = diagnostics.count { it.severity == "error" || it.severity == "fatal error" }
                        val msg = if (errorCount > 0) {
                            "Build failed with $errorCount error(s)"
                        } else {
                            "Build failed (exit code ${result.exitCode})"
                        }
                        NotificationGroupManager.getInstance()
                            .getNotificationGroup(CmodPlugin.NOTIFICATION_GROUP_ID)
                            .createNotification("cmod build", msg, NotificationType.ERROR)
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
