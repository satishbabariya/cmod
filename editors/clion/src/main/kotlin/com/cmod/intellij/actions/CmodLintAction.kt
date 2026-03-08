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
 * Action to run `cmod lint` from the Build menu.
 *
 * Lints all C++ source files in the project using clang-tidy via cmod
 * and reports the number of warnings and errors found.
 */
class CmodLintAction : AnAction() {

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return

        ProgressManager.getInstance().run(object : Task.Backgroundable(project, "cmod lint", true) {
            override fun run(indicator: ProgressIndicator) {
                indicator.text = "Running cmod lint..."
                indicator.isIndeterminate = true

                val result = CmodProcessUtil.runCmodCommand(project, listOf("lint"))

                ApplicationManager.getApplication().invokeLater {
                    val allOutput = result.stdout + result.stderr
                    val diagnostics = CmodProcessUtil.parseClangDiagnostics(allOutput)
                    val errors = diagnostics.count { it.severity == "error" || it.severity == "fatal error" }
                    val warnings = diagnostics.count { it.severity == "warning" }

                    if (result.isSuccess && errors == 0) {
                        val msg = if (warnings > 0) {
                            "Lint passed with $warnings warning(s)"
                        } else {
                            "Lint passed with no issues"
                        }
                        NotificationGroupManager.getInstance()
                            .getNotificationGroup(CmodPlugin.NOTIFICATION_GROUP_ID)
                            .createNotification("cmod lint", msg, NotificationType.INFORMATION)
                            .notify(project)
                    } else {
                        val msg = "Lint found $errors error(s) and $warnings warning(s)"
                        NotificationGroupManager.getInstance()
                            .getNotificationGroup(CmodPlugin.NOTIFICATION_GROUP_ID)
                            .createNotification("cmod lint", msg, NotificationType.WARNING)
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
