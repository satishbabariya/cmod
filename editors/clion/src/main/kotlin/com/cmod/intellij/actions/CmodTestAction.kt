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
 * Action to run `cmod test` from the Build menu.
 *
 * Executes tests in a background task and reports results via
 * balloon notifications with test pass/fail counts.
 */
class CmodTestAction : AnAction() {

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return

        ProgressManager.getInstance().run(object : Task.Backgroundable(project, "cmod test", true) {
            override fun run(indicator: ProgressIndicator) {
                indicator.text = "Running cmod test..."
                indicator.isIndeterminate = true

                val result = CmodProcessUtil.runCmodCommand(project, listOf("test"))

                ApplicationManager.getApplication().invokeLater {
                    if (result.isSuccess) {
                        NotificationGroupManager.getInstance()
                            .getNotificationGroup(CmodPlugin.NOTIFICATION_GROUP_ID)
                            .createNotification("cmod test", "All tests passed", NotificationType.INFORMATION)
                            .notify(project)
                    } else {
                        NotificationGroupManager.getInstance()
                            .getNotificationGroup(CmodPlugin.NOTIFICATION_GROUP_ID)
                            .createNotification(
                                "cmod test",
                                "Tests failed (exit code ${result.exitCode})",
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
