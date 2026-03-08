package com.cmod.intellij.settings

import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.PersistentStateComponent
import com.intellij.openapi.components.State
import com.intellij.openapi.components.Storage

/**
 * Persistent application-level settings for the cmod plugin.
 *
 * Settings are persisted to cmod.xml in the IntelliJ configuration directory.
 * Access via [getInstance].
 */
@State(
    name = "com.cmod.intellij.settings.CmodSettingsState",
    storages = [Storage("cmod.xml")]
)
class CmodSettingsState : PersistentStateComponent<CmodSettingsState> {

    /**
     * Path to the cmod binary. If empty, the plugin searches PATH and
     * common installation directories.
     */
    var cmodBinaryPath: String = ""

    /**
     * Default build profile ("debug" or "release").
     */
    var defaultProfile: String = "debug"

    /**
     * Default number of parallel compilation jobs. 0 means auto-detect
     * based on available CPU cores.
     */
    var defaultJobs: Int = 0

    /**
     * Whether to automatically start the LSP server when a cmod project
     * is opened.
     */
    var autoStartLsp: Boolean = true

    /**
     * Whether to show balloon notifications for build results.
     */
    var showBuildNotifications: Boolean = true

    /**
     * Whether to run `cmod fmt` on file save.
     */
    var formatOnSave: Boolean = false

    /**
     * Whether to run `cmod lint` on file save.
     */
    var lintOnSave: Boolean = false

    companion object {
        fun getInstance(): CmodSettingsState {
            return ApplicationManager.getApplication().getService(CmodSettingsState::class.java)
        }
    }

    override fun getState(): CmodSettingsState = this

    override fun loadState(state: CmodSettingsState) {
        cmodBinaryPath = state.cmodBinaryPath
        defaultProfile = state.defaultProfile
        defaultJobs = state.defaultJobs
        autoStartLsp = state.autoStartLsp
        showBuildNotifications = state.showBuildNotifications
        formatOnSave = state.formatOnSave
        lintOnSave = state.lintOnSave
    }
}
