package com.cmod.intellij.settings

import com.intellij.openapi.options.Configurable
import javax.swing.JComponent

/**
 * Application-level settings configurable for cmod.
 *
 * Appears under Settings > Tools > cmod and allows users to configure:
 * - Path to the cmod binary
 * - Default build profile
 * - Default number of parallel jobs
 * - Whether to auto-start the LSP server
 * - Whether to show build notifications
 */
class CmodSettingsConfigurable : Configurable {

    private var component: CmodSettingsComponent? = null

    override fun getDisplayName(): String = "cmod"

    override fun createComponent(): JComponent {
        val settingsComponent = CmodSettingsComponent()
        component = settingsComponent
        return settingsComponent.getPanel()
    }

    override fun isModified(): Boolean {
        val comp = component ?: return false
        val state = CmodSettingsState.getInstance()

        return comp.cmodBinaryPath != state.cmodBinaryPath ||
                comp.defaultProfile != state.defaultProfile ||
                comp.defaultJobs != state.defaultJobs ||
                comp.autoStartLsp != state.autoStartLsp ||
                comp.showBuildNotifications != state.showBuildNotifications ||
                comp.formatOnSave != state.formatOnSave ||
                comp.lintOnSave != state.lintOnSave
    }

    override fun apply() {
        val comp = component ?: return
        val state = CmodSettingsState.getInstance()

        state.cmodBinaryPath = comp.cmodBinaryPath
        state.defaultProfile = comp.defaultProfile
        state.defaultJobs = comp.defaultJobs
        state.autoStartLsp = comp.autoStartLsp
        state.showBuildNotifications = comp.showBuildNotifications
        state.formatOnSave = comp.formatOnSave
        state.lintOnSave = comp.lintOnSave
    }

    override fun reset() {
        val comp = component ?: return
        val state = CmodSettingsState.getInstance()

        comp.cmodBinaryPath = state.cmodBinaryPath
        comp.defaultProfile = state.defaultProfile
        comp.defaultJobs = state.defaultJobs
        comp.autoStartLsp = state.autoStartLsp
        comp.showBuildNotifications = state.showBuildNotifications
        comp.formatOnSave = state.formatOnSave
        comp.lintOnSave = state.lintOnSave
    }

    override fun disposeUIResources() {
        component = null
    }
}
