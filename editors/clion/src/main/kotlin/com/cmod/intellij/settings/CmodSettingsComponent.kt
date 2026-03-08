package com.cmod.intellij.settings

import com.cmod.intellij.util.CmodBinaryUtil
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.fileChooser.FileChooserDescriptorFactory
import com.intellij.openapi.ui.ComboBox
import com.intellij.openapi.ui.TextFieldWithBrowseButton
import com.intellij.ui.components.JBCheckBox
import com.intellij.ui.components.JBLabel
import com.intellij.util.ui.FormBuilder
import javax.swing.JPanel
import javax.swing.JSpinner
import javax.swing.SpinnerNumberModel

/**
 * Swing form component for cmod settings.
 *
 * Provides UI controls for all cmod plugin settings:
 * - cmod binary path with file browser
 * - Default build profile selector
 * - Parallel jobs spinner
 * - LSP auto-start toggle
 * - Build notification toggle
 * - Format-on-save toggle
 * - Lint-on-save toggle
 */
class CmodSettingsComponent {

    private val binaryPathField = TextFieldWithBrowseButton().apply {
        addBrowseFolderListener(
            "Select cmod Binary",
            "Choose the path to the cmod executable",
            null,
            FileChooserDescriptorFactory.createSingleFileDescriptor()
        )
    }

    private val profileCombo = ComboBox(arrayOf("debug", "release"))
    private val jobsSpinner = JSpinner(SpinnerNumberModel(0, 0, 256, 1))
    private val autoStartLspCheckBox = JBCheckBox("Auto-start LSP server on project open")
    private val showNotificationsCheckBox = JBCheckBox("Show build result notifications")
    private val formatOnSaveCheckBox = JBCheckBox("Run cmod fmt on file save")
    private val lintOnSaveCheckBox = JBCheckBox("Run cmod lint on file save")

    private val versionLabel = JBLabel("")

    private val mainPanel: JPanel

    init {
        // Detect version off the EDT to avoid blocking the UI
        versionLabel.text = "Detecting..."
        ApplicationManager.getApplication().executeOnPooledThread {
            val version = CmodBinaryUtil.getCmodVersion()
            val label = if (version != null) "Detected: $version" else "cmod not found"
            ApplicationManager.getApplication().invokeLater {
                versionLabel.text = label
            }
        }

        mainPanel = FormBuilder.createFormBuilder()
            .addLabeledComponent(JBLabel("cmod binary path:"), binaryPathField)
            .addComponent(versionLabel)
            .addSeparator()
            .addLabeledComponent(JBLabel("Default build profile:"), profileCombo)
            .addLabeledComponent(JBLabel("Default parallel jobs (0 = auto):"), jobsSpinner)
            .addSeparator()
            .addComponent(autoStartLspCheckBox)
            .addComponent(showNotificationsCheckBox)
            .addSeparator()
            .addComponent(formatOnSaveCheckBox)
            .addComponent(lintOnSaveCheckBox)
            .addComponentFillVertically(JPanel(), 0)
            .panel
    }

    fun getPanel(): JPanel = mainPanel

    var cmodBinaryPath: String
        get() = binaryPathField.text
        set(value) { binaryPathField.text = value }

    var defaultProfile: String
        get() = profileCombo.selectedItem as? String ?: "debug"
        set(value) { profileCombo.selectedItem = value }

    var defaultJobs: Int
        get() = jobsSpinner.value as? Int ?: 0
        set(value) { jobsSpinner.value = value }

    var autoStartLsp: Boolean
        get() = autoStartLspCheckBox.isSelected
        set(value) { autoStartLspCheckBox.isSelected = value }

    var showBuildNotifications: Boolean
        get() = showNotificationsCheckBox.isSelected
        set(value) { showNotificationsCheckBox.isSelected = value }

    var formatOnSave: Boolean
        get() = formatOnSaveCheckBox.isSelected
        set(value) { formatOnSaveCheckBox.isSelected = value }

    var lintOnSave: Boolean
        get() = lintOnSaveCheckBox.isSelected
        set(value) { lintOnSaveCheckBox.isSelected = value }
}
