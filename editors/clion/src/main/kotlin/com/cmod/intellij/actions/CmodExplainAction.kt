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
import com.intellij.openapi.ui.Messages

/**
 * Action to run `cmod explain <module>` from the Build menu.
 *
 * Prompts the user for a module name and then explains why that module
 * would be rebuilt, displaying the result in a dialog.
 */
class CmodExplainAction : AnAction() {

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return

        val moduleName = Messages.showInputDialog(
            project,
            "Enter the module name to explain:",
            "cmod explain",
            null
        )

        if (moduleName.isNullOrBlank()) return

        ProgressManager.getInstance().run(object : Task.Backgroundable(project, "cmod explain", true) {
            override fun run(indicator: ProgressIndicator) {
                indicator.text = "Running cmod explain $moduleName..."
                indicator.isIndeterminate = true

                val result = CmodProcessUtil.runCmodCommand(project, listOf("explain", moduleName))

                ApplicationManager.getApplication().invokeLater {
                    if (result.isSuccess) {
                        val output = result.stdout.ifBlank { "Module $moduleName is up to date." }
                        Messages.showInfoMessage(
                            project,
                            output,
                            "cmod explain: $moduleName"
                        )
                    } else {
                        NotificationGroupManager.getInstance()
                            .getNotificationGroup(CmodPlugin.NOTIFICATION_GROUP_ID)
                            .createNotification(
                                "cmod explain",
                                "Failed to explain module $moduleName: ${result.stderr.take(200)}",
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
