package com.cmod.intellij.runConfig

import com.intellij.openapi.options.SettingsEditor
import com.intellij.openapi.ui.ComboBox
import com.intellij.ui.components.JBCheckBox
import com.intellij.ui.components.JBLabel
import com.intellij.util.ui.FormBuilder
import javax.swing.JComponent
import javax.swing.JPanel
import javax.swing.JSpinner
import javax.swing.SpinnerNumberModel

/**
 * Swing UI editor for cmod Build run configuration settings.
 *
 * Provides controls for:
 * - Build profile (debug/release)
 * - Parallel jobs count
 * - Force rebuild toggle
 * - Timing display toggle
 */
class CmodBuildConfigEditor : SettingsEditor<CmodBuildRunConfiguration>() {

    private val profileCombo = ComboBox(arrayOf("debug", "release"))
    private val jobsSpinner = JSpinner(SpinnerNumberModel(0, 0, 256, 1))
    private val forceCheckBox = JBCheckBox("Force rebuild all modules")
    private val timingsCheckBox = JBCheckBox("Show per-module timing")

    private lateinit var mainPanel: JPanel

    init {
        mainPanel = FormBuilder.createFormBuilder()
            .addLabeledComponent(JBLabel("Build profile:"), profileCombo)
            .addLabeledComponent(JBLabel("Parallel jobs (0 = auto):"), jobsSpinner)
            .addComponent(forceCheckBox)
            .addComponent(timingsCheckBox)
            .addComponentFillVertically(JPanel(), 0)
            .panel
    }

    override fun resetEditorFrom(config: CmodBuildRunConfiguration) {
        profileCombo.selectedItem = config.profile
        jobsSpinner.value = config.jobs
        forceCheckBox.isSelected = config.force
        timingsCheckBox.isSelected = config.timings
    }

    override fun applyEditorTo(config: CmodBuildRunConfiguration) {
        config.profile = profileCombo.selectedItem as? String ?: "debug"
        config.jobs = jobsSpinner.value as? Int ?: 0
        config.force = forceCheckBox.isSelected
        config.timings = timingsCheckBox.isSelected
    }

    override fun createEditor(): JComponent = mainPanel
}
