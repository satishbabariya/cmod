package com.cmod.intellij.toolWindow

import com.cmod.intellij.util.CmodManifestUtil
import com.intellij.icons.AllIcons
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.intellij.ui.components.JBLabel
import com.intellij.ui.components.JBScrollPane
import com.intellij.ui.treeStructure.Tree
import com.intellij.util.ui.JBUI
import java.awt.BorderLayout
import javax.swing.*
import javax.swing.tree.DefaultMutableTreeNode
import javax.swing.tree.DefaultTreeCellRenderer
import javax.swing.tree.DefaultTreeModel

/**
 * Panel displaying the dependency tree from cmod.toml.
 *
 * Reads the project manifest and displays:
 * - Package metadata (name, version, edition)
 * - Module information
 * - Dependencies with version constraints
 * - Workspace members (if applicable)
 */
class DependencyTreePanel(private val project: Project) {

    companion object {
        private val LOG = Logger.getInstance(DependencyTreePanel::class.java)
    }

    private val rootPanel = JPanel(BorderLayout())
    private val statusLabel = JBLabel("")
    private var tree: Tree? = null

    init {
        val toolbar = JPanel().apply {
            layout = BoxLayout(this, BoxLayout.X_AXIS)
            border = JBUI.Borders.empty(4)

            val refreshButton = JButton("Refresh").apply {
                icon = AllIcons.Actions.Refresh
                addActionListener { loadDependencies() }
            }
            add(refreshButton)
            add(Box.createHorizontalGlue())
            add(statusLabel)
        }

        rootPanel.add(toolbar, BorderLayout.NORTH)
        rootPanel.add(JBLabel("Click Refresh to load dependencies", SwingConstants.CENTER), BorderLayout.CENTER)

        // Auto-load on creation
        loadDependencies()
    }

    fun getComponent(): JComponent = rootPanel

    private fun loadDependencies() {
        statusLabel.text = "Loading..."

        ApplicationManager.getApplication().executeOnPooledThread {
            val manifest = CmodManifestUtil.parseManifest(project)

            ApplicationManager.getApplication().invokeLater {
                if (manifest == null) {
                    statusLabel.text = "No cmod.toml found"
                    displayEmptyState("No cmod.toml found in project root.")
                    return@invokeLater
                }

                displayDependencyTree(manifest)
                statusLabel.text = "${manifest.dependencies.size} dependencies"
            }
        }
    }

    private fun displayDependencyTree(manifest: CmodManifestUtil.ManifestInfo) {
        val projectName = manifest.name.ifBlank { project.name }
        val rootNode = DefaultMutableTreeNode("$projectName ${manifest.version}")

        // Package info
        if (manifest.name.isNotBlank() || manifest.edition.isNotBlank()) {
            val packageNode = DefaultMutableTreeNode("Package")
            if (manifest.name.isNotBlank()) {
                packageNode.add(DefaultMutableTreeNode("name: ${manifest.name}"))
            }
            if (manifest.version.isNotBlank()) {
                packageNode.add(DefaultMutableTreeNode("version: ${manifest.version}"))
            }
            if (manifest.edition.isNotBlank()) {
                packageNode.add(DefaultMutableTreeNode("edition: ${manifest.edition}"))
            }
            rootNode.add(packageNode)
        }

        // Module info
        if (manifest.moduleName.isNotBlank()) {
            val moduleNode = DefaultMutableTreeNode("Module")
            moduleNode.add(DefaultMutableTreeNode("name: ${manifest.moduleName}"))
            rootNode.add(moduleNode)
        }

        // Build info
        if (manifest.buildType.isNotBlank()) {
            val buildNode = DefaultMutableTreeNode("Build")
            buildNode.add(DefaultMutableTreeNode("type: ${manifest.buildType}"))
            rootNode.add(buildNode)
        }

        // Dependencies
        if (manifest.dependencies.isNotEmpty()) {
            val depsNode = DefaultMutableTreeNode("Dependencies (${manifest.dependencies.size})")
            for ((name, version) in manifest.dependencies.entries.sortedBy { it.key }) {
                depsNode.add(DefaultMutableTreeNode("$name = \"$version\""))
            }
            rootNode.add(depsNode)
        }

        // Workspace members
        if (manifest.workspaceMembers.isNotEmpty()) {
            val wsNode = DefaultMutableTreeNode("Workspace Members (${manifest.workspaceMembers.size})")
            for (member in manifest.workspaceMembers) {
                wsNode.add(DefaultMutableTreeNode(member))
            }
            rootNode.add(wsNode)
        }

        val treeModel = DefaultTreeModel(rootNode)
        val newTree = Tree(treeModel).apply {
            isRootVisible = true
            showsRootHandles = true
            cellRenderer = DependencyTreeCellRenderer()
        }

        // Expand all nodes
        for (i in 0 until newTree.rowCount) {
            newTree.expandRow(i)
        }

        tree = newTree
        val scrollPane = JBScrollPane(newTree)

        // Replace center component
        val centerComponent = rootPanel.getComponent(1)
        rootPanel.remove(centerComponent)
        rootPanel.add(scrollPane, BorderLayout.CENTER)
        rootPanel.revalidate()
        rootPanel.repaint()
    }

    private fun displayEmptyState(message: String) {
        val centerComponent = rootPanel.getComponent(1)
        rootPanel.remove(centerComponent)
        rootPanel.add(JBLabel(message, SwingConstants.CENTER), BorderLayout.CENTER)
        rootPanel.revalidate()
        rootPanel.repaint()
    }

    /**
     * Custom cell renderer with icons for different node types.
     */
    private class DependencyTreeCellRenderer : DefaultTreeCellRenderer() {
        override fun getTreeCellRendererComponent(
            tree: JTree?,
            value: Any?,
            sel: Boolean,
            expanded: Boolean,
            leaf: Boolean,
            row: Int,
            hasFocus: Boolean
        ): java.awt.Component {
            super.getTreeCellRendererComponent(tree, value, sel, expanded, leaf, row, hasFocus)

            val nodeText = value?.toString() ?: ""
            when {
                nodeText.startsWith("Package") -> icon = AllIcons.Nodes.Package
                nodeText.startsWith("Module") -> icon = AllIcons.Nodes.Module
                nodeText.startsWith("Build") -> icon = AllIcons.Actions.Compile
                nodeText.startsWith("Dependencies") -> icon = AllIcons.Nodes.PpLibFolder
                nodeText.startsWith("Workspace") -> icon = AllIcons.Nodes.Folder
                nodeText.contains("=") -> icon = AllIcons.Nodes.PpLib
                nodeText.contains(":") && leaf -> icon = AllIcons.Nodes.Property
            }

            return this
        }
    }
}
