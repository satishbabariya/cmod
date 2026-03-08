package com.cmod.intellij.runConfig

import com.intellij.execution.configurations.ConfigurationFactory
import com.intellij.execution.configurations.ConfigurationType
import com.intellij.execution.configurations.RunConfiguration
import com.intellij.icons.AllIcons
import com.intellij.openapi.project.Project
import javax.swing.Icon

/**
 * Run configuration type for `cmod run`.
 *
 * Registers a "cmod Run" entry in the run configuration dropdown,
 * allowing users to build and run their project binary with arguments.
 */
class CmodRunConfigurationType : ConfigurationType {

    companion object {
        const val ID = "CmodRunRunConfiguration"
    }

    override fun getDisplayName(): String = "cmod Run"

    override fun getConfigurationTypeDescription(): String =
        "Build and run a cmod C++ module project"

    override fun getIcon(): Icon = AllIcons.Actions.Execute

    override fun getId(): String = ID

    override fun getConfigurationFactories(): Array<ConfigurationFactory> {
        return arrayOf(CmodRunConfigurationFactory(this))
    }

    private class CmodRunConfigurationFactory(type: ConfigurationType) : ConfigurationFactory(type) {

        override fun getId(): String = "CmodRunConfigurationFactory"

        override fun getName(): String = "cmod Run"

        override fun createTemplateConfiguration(project: Project): RunConfiguration {
            return CmodRunConfiguration(project, this, "cmod Run")
        }
    }
}
