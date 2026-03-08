package com.cmod.intellij.toolWindow

import com.cmod.intellij.CmodPlugin
import com.intellij.openapi.project.DumbAware
import com.intellij.openapi.project.Project
import com.intellij.openapi.wm.ToolWindow
import com.intellij.openapi.wm.ToolWindowFactory
import com.intellij.ui.content.ContentFactory

/**
 * Factory for the cmod tool window.
 *
 * Creates a three-tab tool window anchored to the right side of the IDE:
 * 1. Module Graph - visualizes the module dependency graph
 * 2. Dependencies - displays the dependency tree from cmod.toml
 * 3. Cache - shows cache statistics with refresh and clean controls
 */
class CmodToolWindowFactory : ToolWindowFactory, DumbAware {

    override fun isApplicable(project: Project): Boolean {
        return CmodPlugin.isCmodProject(project)
    }

    override fun createToolWindowContent(project: Project, toolWindow: ToolWindow) {
        val contentFactory = ContentFactory.getInstance()

        // Tab 1: Module Graph
        val graphPanel = ModuleGraphPanel(project)
        val graphContent = contentFactory.createContent(
            graphPanel.getComponent(),
            "Module Graph",
            false
        )
        graphContent.isCloseable = false
        toolWindow.contentManager.addContent(graphContent)

        // Tab 2: Dependencies
        val depsPanel = DependencyTreePanel(project)
        val depsContent = contentFactory.createContent(
            depsPanel.getComponent(),
            "Dependencies",
            false
        )
        depsContent.isCloseable = false
        toolWindow.contentManager.addContent(depsContent)

        // Tab 3: Cache Status
        val cachePanel = CacheStatusPanel(project)
        val cacheContent = contentFactory.createContent(
            cachePanel.getComponent(),
            "Cache",
            false
        )
        cacheContent.isCloseable = false
        toolWindow.contentManager.addContent(cacheContent)
    }
}
