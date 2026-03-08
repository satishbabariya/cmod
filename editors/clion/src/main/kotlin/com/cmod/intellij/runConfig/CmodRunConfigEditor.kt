package com.cmod.intellij.runConfig

import com.intellij.openapi.options.SettingsEditor
import com.intellij.openapi.ui.ComboBox
import com.intellij.ui.components.JBLabel
import com.intellij.ui.components.JBTextField
import com.intellij.util.ui.FormBuilder
import javax.swing.JComponent
import javax.swing.JPanel

/**
 * Swing UI editor for cmod Run configuration settings.
 *
 * Provides controls for:
 * - Build profile (debug/release)
 * - Program arguments (passed after --)
 */
class CmodRunConfigEditor : SettingsEditor<CmodRunConfiguration>() {

    private val profileCombo = ComboBox(arrayOf("debug", "release"))
    private val argumentsField = JBTextField()

    private lateinit var mainPanel: JPanel

    init {
        mainPanel = FormBuilder.createFormBuilder()
            .addLabeledComponent(JBLabel("Build profile:"), profileCombo)
            .addLabeledComponent(JBLabel("Program arguments:"), argumentsField)
            .addComponentFillVertically(JPanel(), 0)
            .panel
    }

    override fun resetEditorFrom(config: CmodRunConfiguration) {
        profileCombo.selectedItem = config.profile
        argumentsField.text = config.arguments
    }

    override fun applyEditorTo(config: CmodRunConfiguration) {
        config.profile = profileCombo.selectedItem as? String ?: "debug"
        config.arguments = argumentsField.text
    }

    override fun createEditor(): JComponent = mainPanel
}
