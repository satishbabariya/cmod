package com.cmod.intellij.actions

import com.cmod.intellij.CmodPlugin
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.wm.ToolWindowManager

/**
 * Action to show the module dependency graph in the cmod tool window.
 *
 * Opens the cmod tool window and activates the "Module Graph" tab,
 * triggering a refresh of the graph data from `cmod graph --format json`.
 */
class CmodShowGraphAction : AnAction() {

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return

        val toolWindow = ToolWindowManager.getInstance(project).getToolWindow("cmod")
        if (toolWindow != null) {
            toolWindow.show {
                // Activate the first content (Module Graph tab)
                val contentManager = toolWindow.contentManager
                val graphContent = contentManager.contents.firstOrNull()
                if (graphContent != null) {
                    contentManager.setSelectedContent(graphContent)
                }
            }
        }
    }

    override fun update(e: AnActionEvent) {
        val project = e.project
        e.presentation.isEnabledAndVisible = project != null && CmodPlugin.isCmodProject(project)
    }
}
