package com.cmod.intellij.toolWindow

import com.cmod.intellij.CmodPlugin
import com.cmod.intellij.util.CmodProcessUtil
import com.intellij.icons.AllIcons
import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.ProgressManager
import com.intellij.openapi.progress.Task
import com.intellij.ui.JBColor
import com.intellij.ui.components.JBLabel
import com.intellij.ui.components.JBScrollPane
import com.intellij.util.ui.JBUI
import java.awt.BorderLayout
import java.awt.GridBagConstraints
import java.awt.GridBagLayout
import java.awt.Insets
import javax.swing.*

/**
 * Panel displaying artifact cache statistics.
 *
 * Shows cache location, total size, entry count, hit/miss rates,
 * and provides buttons for refreshing and cleaning the cache.
 */
class CacheStatusPanel(private val project: Project) {

    companion object {
        private val LOG = Logger.getInstance(CacheStatusPanel::class.java)
    }

    private val rootPanel = JPanel(BorderLayout())
    private val statusLabel = JBLabel("")
    private val statsPanel = JPanel(GridBagLayout())

    private val locationValue = JBLabel("-")
    private val totalSizeValue = JBLabel("-")
    private val entryCountValue = JBLabel("-")
    private val hitRateValue = JBLabel("-")
    private val lastCleanedValue = JBLabel("-")

    init {
        val toolbar = JPanel().apply {
            layout = BoxLayout(this, BoxLayout.X_AXIS)
            border = JBUI.Borders.empty(4)

            val refreshButton = JButton("Refresh").apply {
                icon = AllIcons.Actions.Refresh
                addActionListener { loadCacheStatus() }
            }
            add(refreshButton)

            add(Box.createHorizontalStrut(8))

            val cleanButton = JButton("Clean").apply {
                icon = AllIcons.Actions.GC
                addActionListener { cleanCache() }
            }
            add(cleanButton)

            add(Box.createHorizontalStrut(8))

            val gcButton = JButton("GC").apply {
                icon = AllIcons.Actions.ProjectWideAnalysisOff
                toolTipText = "Run garbage collection on expired cache entries"
                addActionListener { gcCache() }
            }
            add(gcButton)

            add(Box.createHorizontalGlue())
            add(statusLabel)
        }

        buildStatsPanel()

        rootPanel.add(toolbar, BorderLayout.NORTH)
        rootPanel.add(JBScrollPane(statsPanel), BorderLayout.CENTER)

        // Auto-load on creation
        loadCacheStatus()
    }

    fun getComponent(): JComponent = rootPanel

    private fun buildStatsPanel() {
        statsPanel.border = JBUI.Borders.empty(12)
        val gbc = GridBagConstraints().apply {
            fill = GridBagConstraints.HORIZONTAL
            insets = Insets(4, 8, 4, 8)
        }

        var row = 0

        // Title
        gbc.gridx = 0; gbc.gridy = row; gbc.gridwidth = 2; gbc.anchor = GridBagConstraints.WEST
        val titleLabel = JBLabel("Artifact Cache").apply {
            font = font.deriveFont(font.size * 1.3f)
            foreground = JBColor.foreground()
        }
        statsPanel.add(titleLabel, gbc)
        gbc.gridwidth = 1
        row++

        // Separator
        gbc.gridx = 0; gbc.gridy = row; gbc.gridwidth = 2
        statsPanel.add(JSeparator(), gbc)
        gbc.gridwidth = 1
        row++

        // Cache location
        addStatRow(row++, "Location:", locationValue, gbc)

        // Total size
        addStatRow(row++, "Total size:", totalSizeValue, gbc)

        // Entry count
        addStatRow(row++, "Entries:", entryCountValue, gbc)

        // Hit rate
        addStatRow(row++, "Hit rate:", hitRateValue, gbc)

        // Last cleaned
        addStatRow(row++, "Last cleaned:", lastCleanedValue, gbc)

        // Spacer
        gbc.gridx = 0; gbc.gridy = row; gbc.weighty = 1.0; gbc.gridwidth = 2
        statsPanel.add(JPanel(), gbc)
    }

    private fun addStatRow(row: Int, label: String, valueLabel: JBLabel, gbc: GridBagConstraints) {
        gbc.gridx = 0; gbc.gridy = row; gbc.weightx = 0.0; gbc.anchor = GridBagConstraints.WEST
        val labelComponent = JBLabel(label).apply {
            foreground = JBColor.gray
        }
        statsPanel.add(labelComponent, gbc)

        gbc.gridx = 1; gbc.weightx = 1.0
        statsPanel.add(valueLabel, gbc)
    }

    private fun loadCacheStatus() {
        statusLabel.text = "Loading..."

        ProgressManager.getInstance().run(object : Task.Backgroundable(project, "Loading cache status", false) {
            override fun run(indicator: ProgressIndicator) {
                val result = CmodProcessUtil.runCmodCommand(
                    project,
                    listOf("cache", "status"),
                    timeoutSeconds = 15
                )

                ApplicationManager.getApplication().invokeLater {
                    if (result.isSuccess) {
                        parseCacheStatusOutput(result.stdout)
                        statusLabel.text = "Updated"
                    } else {
                        statusLabel.text = "Failed"
                        locationValue.text = "Error loading cache status"
                        totalSizeValue.text = "-"
                        entryCountValue.text = "-"
                        hitRateValue.text = "-"
                        lastCleanedValue.text = "-"
                    }
                }
            }
        })
    }

    private fun parseCacheStatusOutput(output: String) {
        // Parse the text output from `cmod cache status`
        // Expected format is key-value pairs, one per line
        for (line in output.lines()) {
            val trimmed = line.trim()
            when {
                trimmed.startsWith("Location:", ignoreCase = true) ||
                trimmed.startsWith("Cache path:", ignoreCase = true) ->
                    locationValue.text = trimmed.substringAfter(":").trim()

                trimmed.startsWith("Total size:", ignoreCase = true) ||
                trimmed.startsWith("Size:", ignoreCase = true) ->
                    totalSizeValue.text = trimmed.substringAfter(":").trim()

                trimmed.startsWith("Entries:", ignoreCase = true) ||
                trimmed.startsWith("Entry count:", ignoreCase = true) ||
                trimmed.startsWith("Artifacts:", ignoreCase = true) ->
                    entryCountValue.text = trimmed.substringAfter(":").trim()

                trimmed.startsWith("Hit rate:", ignoreCase = true) ||
                trimmed.startsWith("Cache hit rate:", ignoreCase = true) ->
                    hitRateValue.text = trimmed.substringAfter(":").trim()

                trimmed.startsWith("Last cleaned:", ignoreCase = true) ||
                trimmed.startsWith("Last GC:", ignoreCase = true) ->
                    lastCleanedValue.text = trimmed.substringAfter(":").trim()
            }
        }
    }

    private fun cleanCache() {
        ProgressManager.getInstance().run(object : Task.Backgroundable(project, "Cleaning cache", true) {
            override fun run(indicator: ProgressIndicator) {
                indicator.text = "Running cmod cache clean..."
                val result = CmodProcessUtil.runCmodCommand(project, listOf("cache", "clean"))

                ApplicationManager.getApplication().invokeLater {
                    if (result.isSuccess) {
                        NotificationGroupManager.getInstance()
                            .getNotificationGroup(CmodPlugin.NOTIFICATION_GROUP_ID)
                            .createNotification("cmod cache", "Cache cleaned successfully", NotificationType.INFORMATION)
                            .notify(project)
                        loadCacheStatus() // Refresh the stats
                    } else {
                        NotificationGroupManager.getInstance()
                            .getNotificationGroup(CmodPlugin.NOTIFICATION_GROUP_ID)
                            .createNotification(
                                "cmod cache",
                                "Failed to clean cache: ${result.stderr.take(200)}",
                                NotificationType.ERROR
                            )
                            .notify(project)
                    }
                }
            }
        })
    }

    private fun gcCache() {
        ProgressManager.getInstance().run(object : Task.Backgroundable(project, "Cache GC", true) {
            override fun run(indicator: ProgressIndicator) {
                indicator.text = "Running cmod cache gc..."
                val result = CmodProcessUtil.runCmodCommand(project, listOf("cache", "gc"))

                ApplicationManager.getApplication().invokeLater {
                    if (result.isSuccess) {
                        NotificationGroupManager.getInstance()
                            .getNotificationGroup(CmodPlugin.NOTIFICATION_GROUP_ID)
                            .createNotification("cmod cache", "Garbage collection complete", NotificationType.INFORMATION)
                            .notify(project)
                        loadCacheStatus()
                    } else {
                        NotificationGroupManager.getInstance()
                            .getNotificationGroup(CmodPlugin.NOTIFICATION_GROUP_ID)
                            .createNotification(
                                "cmod cache",
                                "GC failed: ${result.stderr.take(200)}",
                                NotificationType.ERROR
                            )
                            .notify(project)
                    }
                }
            }
        })
    }
}
