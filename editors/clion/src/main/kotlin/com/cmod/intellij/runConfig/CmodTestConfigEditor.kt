package com.cmod.intellij.runConfig

import com.intellij.openapi.options.SettingsEditor
import com.intellij.openapi.ui.ComboBox
import com.intellij.ui.components.JBCheckBox
import com.intellij.ui.components.JBLabel
import com.intellij.ui.components.JBTextField
import com.intellij.util.ui.FormBuilder
import javax.swing.JComponent
import javax.swing.JPanel

/**
 * Swing UI editor for cmod Test run configuration settings.
 *
 * Provides controls for:
 * - Test name filter pattern
 * - Build profile (debug/release)
 * - Code coverage toggle
 * - Sanitizer selection (none, address, thread, undefined, memory)
 */
class CmodTestConfigEditor : SettingsEditor<CmodTestRunConfiguration>() {

    private val filterField = JBTextField()
    private val profileCombo = ComboBox(arrayOf("debug", "release"))
    private val coverageCheckBox = JBCheckBox("Enable code coverage")
    private val sanitizerCombo = ComboBox(arrayOf("none", "address", "thread", "undefined", "memory"))

    private lateinit var mainPanel: JPanel

    init {
        mainPanel = FormBuilder.createFormBuilder()
            .addLabeledComponent(JBLabel("Test filter:"), filterField)
            .addLabeledComponent(JBLabel("Build profile:"), profileCombo)
            .addComponent(coverageCheckBox)
            .addLabeledComponent(JBLabel("Sanitizer:"), sanitizerCombo)
            .addComponentFillVertically(JPanel(), 0)
            .panel
    }

    override fun resetEditorFrom(config: CmodTestRunConfiguration) {
        filterField.text = config.filter
        profileCombo.selectedItem = config.profile
        coverageCheckBox.isSelected = config.coverage
        sanitizerCombo.selectedItem = config.sanitizer
    }

    override fun applyEditorTo(config: CmodTestRunConfiguration) {
        config.filter = filterField.text
        config.profile = profileCombo.selectedItem as? String ?: "debug"
        config.coverage = coverageCheckBox.isSelected
        config.sanitizer = sanitizerCombo.selectedItem as? String ?: "none"
    }

    override fun createEditor(): JComponent = mainPanel
}
