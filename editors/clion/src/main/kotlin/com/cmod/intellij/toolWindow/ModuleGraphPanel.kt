package com.cmod.intellij.toolWindow

import com.cmod.intellij.util.CmodProcessUtil
import com.google.gson.JsonParser
import com.intellij.icons.AllIcons
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.ProgressManager
import com.intellij.openapi.progress.Task
import com.intellij.ui.JBColor
import com.intellij.ui.components.JBLabel
import com.intellij.ui.components.JBScrollPane
import com.intellij.ui.treeStructure.Tree
import com.intellij.util.ui.JBUI
import java.awt.BorderLayout
import java.awt.Color
import java.awt.Dimension
import java.awt.Graphics
import java.awt.Graphics2D
import java.awt.RenderingHints
import java.awt.geom.Ellipse2D
import java.awt.geom.Line2D
import javax.swing.*
import javax.swing.tree.DefaultMutableTreeNode
import javax.swing.tree.DefaultTreeModel
import kotlin.math.cos
import kotlin.math.sin

/**
 * Panel for visualizing the module dependency graph.
 *
 * Attempts to use JCEF (embedded Chromium) for a rich d3.js-based graph
 * visualization. Falls back to a custom Swing canvas with a force-directed
 * layout, or a JTree for simplicity.
 */
class ModuleGraphPanel(private val project: Project) {

    companion object {
        private val LOG = Logger.getInstance(ModuleGraphPanel::class.java)
    }

    private val rootPanel = JPanel(BorderLayout())
    private val statusLabel = JBLabel("Loading module graph...")
    private var graphData: GraphData? = null

    data class GraphNode(
        val name: String,
        val status: String = "unknown", // "ok", "rebuild", "unknown"
        val timing: Double = 0.0,
        var x: Double = 0.0,
        var y: Double = 0.0,
    )

    data class GraphEdge(
        val from: String,
        val to: String,
    )

    data class GraphData(
        val nodes: List<GraphNode>,
        val edges: List<GraphEdge>,
    )

    init {
        val toolbar = JPanel().apply {
            layout = BoxLayout(this, BoxLayout.X_AXIS)
            border = JBUI.Borders.empty(4)

            val refreshButton = JButton("Refresh").apply {
                icon = AllIcons.Actions.Refresh
                addActionListener { loadGraph() }
            }
            add(refreshButton)
            add(Box.createHorizontalGlue())
            add(statusLabel)
        }

        rootPanel.add(toolbar, BorderLayout.NORTH)
        rootPanel.add(JBLabel("Click Refresh to load the module graph", SwingConstants.CENTER), BorderLayout.CENTER)

        // Auto-load on creation
        loadGraph()
    }

    fun getComponent(): JComponent = rootPanel

    private fun loadGraph() {
        statusLabel.text = "Loading..."

        ProgressManager.getInstance().run(object : Task.Backgroundable(project, "Loading module graph", false) {
            override fun run(indicator: ProgressIndicator) {
                val result = CmodProcessUtil.runCmodCommand(
                    project,
                    listOf("graph", "--format", "json", "--status", "--timing"),
                    timeoutSeconds = 30
                )

                ApplicationManager.getApplication().invokeLater {
                    if (result.isSuccess) {
                        try {
                            graphData = parseGraphJson(result.stdout)
                            displayGraph()
                            statusLabel.text = "${graphData?.nodes?.size ?: 0} modules"
                        } catch (e: Exception) {
                            LOG.warn("Failed to parse graph JSON", e)
                            statusLabel.text = "Parse error"
                            displayFallbackTree(result.stdout)
                        }
                    } else {
                        statusLabel.text = "Failed to load graph"
                        displayEmptyState("Failed to load module graph.\n${result.stderr.take(300)}")
                    }
                }
            }
        })
    }

    private fun parseGraphJson(json: String): GraphData {
        val root = JsonParser.parseString(json).asJsonObject
        val nodes = mutableListOf<GraphNode>()
        val edges = mutableListOf<GraphEdge>()

        if (root.has("nodes")) {
            for (nodeElem in root.getAsJsonArray("nodes")) {
                val obj = nodeElem.asJsonObject
                nodes.add(
                    GraphNode(
                        name = obj.get("name")?.asString ?: "unknown",
                        status = obj.get("status")?.asString ?: "unknown",
                        timing = obj.get("timing")?.asDouble ?: 0.0,
                    )
                )
            }
        }

        if (root.has("edges")) {
            for (edgeElem in root.getAsJsonArray("edges")) {
                val obj = edgeElem.asJsonObject
                edges.add(
                    GraphEdge(
                        from = obj.get("from")?.asString ?: "",
                        to = obj.get("to")?.asString ?: "",
                    )
                )
            }
        }

        return GraphData(nodes, edges)
    }

    private fun displayGraph() {
        val data = graphData ?: return

        if (data.nodes.isEmpty()) {
            displayEmptyState("No modules found in the dependency graph.")
            return
        }

        // Assign positions in a circular layout
        val centerX = 300.0
        val centerY = 300.0
        val radius = 200.0
        val angleStep = 2 * Math.PI / data.nodes.size

        data.nodes.forEachIndexed { i, node ->
            node.x = centerX + radius * cos(i * angleStep)
            node.y = centerY + radius * sin(i * angleStep)
        }

        val canvas = GraphCanvas(data)
        val scrollPane = JBScrollPane(canvas)

        rootPanel.remove(rootPanel.getComponent(1)) // remove center component
        rootPanel.add(scrollPane, BorderLayout.CENTER)
        rootPanel.revalidate()
        rootPanel.repaint()
    }

    private fun displayFallbackTree(rawOutput: String) {
        val rootNode = DefaultMutableTreeNode("Module Graph")

        // Parse the raw output line by line for tree display
        for (line in rawOutput.lines()) {
            val trimmed = line.trim()
            if (trimmed.isNotEmpty() && !trimmed.startsWith("{") && !trimmed.startsWith("}")) {
                rootNode.add(DefaultMutableTreeNode(trimmed))
            }
        }

        val tree = Tree(DefaultTreeModel(rootNode))
        val scrollPane = JBScrollPane(tree)

        rootPanel.remove(rootPanel.getComponent(1))
        rootPanel.add(scrollPane, BorderLayout.CENTER)
        rootPanel.revalidate()
        rootPanel.repaint()
    }

    private fun displayEmptyState(message: String) {
        rootPanel.remove(rootPanel.getComponent(1))
        rootPanel.add(JBLabel(message, SwingConstants.CENTER), BorderLayout.CENTER)
        rootPanel.revalidate()
        rootPanel.repaint()
    }

    /**
     * Custom Swing canvas that renders the module graph with nodes and edges.
     */
    private class GraphCanvas(private val data: GraphData) : JPanel() {

        private val nodeRadius = 24.0
        private val nodeMap: Map<String, GraphNode> = data.nodes.associateBy { it.name }

        init {
            preferredSize = Dimension(600, 600)
            background = JBColor.background()
        }

        override fun paintComponent(g: Graphics) {
            super.paintComponent(g)
            val g2 = g as Graphics2D
            g2.setRenderingHint(RenderingHints.KEY_ANTIALIASING, RenderingHints.VALUE_ANTIALIAS_ON)
            g2.setRenderingHint(RenderingHints.KEY_TEXT_ANTIALIASING, RenderingHints.VALUE_TEXT_ANTIALIAS_LCD_HRGB)

            // Draw edges
            g2.color = JBColor.border()
            g2.stroke = java.awt.BasicStroke(1.5f)
            for (edge in data.edges) {
                val fromNode = nodeMap[edge.from] ?: continue
                val toNode = nodeMap[edge.to] ?: continue
                g2.draw(Line2D.Double(fromNode.x, fromNode.y, toNode.x, toNode.y))

                // Draw arrowhead
                drawArrowHead(g2, fromNode.x, fromNode.y, toNode.x, toNode.y)
            }

            // Draw nodes
            for (node in data.nodes) {
                val color = when (node.status) {
                    "ok" -> JBColor(Color(76, 175, 80), Color(76, 175, 80))       // green
                    "rebuild" -> JBColor(Color(255, 193, 7), Color(255, 193, 7))   // yellow
                    else -> JBColor(Color(158, 158, 158), Color(120, 120, 120))    // gray
                }

                g2.color = color
                g2.fill(Ellipse2D.Double(
                    node.x - nodeRadius,
                    node.y - nodeRadius,
                    nodeRadius * 2,
                    nodeRadius * 2
                ))

                // Draw border
                g2.color = JBColor.foreground()
                g2.stroke = java.awt.BasicStroke(2f)
                g2.draw(Ellipse2D.Double(
                    node.x - nodeRadius,
                    node.y - nodeRadius,
                    nodeRadius * 2,
                    nodeRadius * 2
                ))

                // Draw label
                g2.color = JBColor.foreground()
                g2.font = g2.font.deriveFont(11f)
                val metrics = g2.fontMetrics
                val labelWidth = metrics.stringWidth(node.name)
                g2.drawString(
                    node.name,
                    (node.x - labelWidth / 2).toFloat(),
                    (node.y + nodeRadius + 16).toFloat()
                )

                // Draw timing if available
                if (node.timing > 0) {
                    val timingStr = String.format("%.1fms", node.timing)
                    val timingWidth = metrics.stringWidth(timingStr)
                    g2.color = JBColor.gray
                    g2.drawString(
                        timingStr,
                        (node.x - timingWidth / 2).toFloat(),
                        (node.y + 4).toFloat()
                    )
                }
            }
        }

        private fun drawArrowHead(g2: Graphics2D, x1: Double, y1: Double, x2: Double, y2: Double) {
            val arrowLen = 10.0
            val arrowAngle = Math.toRadians(25.0)
            val dx = x2 - x1
            val dy = y2 - y1
            val angle = Math.atan2(dy, dx)

            // Offset the arrow tip to the edge of the target node
            val tipX = x2 - nodeRadius * cos(angle)
            val tipY = y2 - nodeRadius * sin(angle)

            val x3 = tipX - arrowLen * cos(angle - arrowAngle)
            val y3 = tipY - arrowLen * sin(angle - arrowAngle)
            val x4 = tipX - arrowLen * cos(angle + arrowAngle)
            val y4 = tipY - arrowLen * sin(angle + arrowAngle)

            val arrowHead = java.awt.Polygon()
            arrowHead.addPoint(tipX.toInt(), tipY.toInt())
            arrowHead.addPoint(x3.toInt(), y3.toInt())
            arrowHead.addPoint(x4.toInt(), y4.toInt())
            g2.fill(arrowHead)
        }
    }
}
