package com.cmod.intellij.runConfig

import com.intellij.execution.configurations.ConfigurationFactory
import com.intellij.execution.configurations.ConfigurationType
import com.intellij.execution.configurations.RunConfiguration
import com.intellij.icons.AllIcons
import com.intellij.openapi.project.Project
import javax.swing.Icon

/**
 * Run configuration type for `cmod build`.
 *
 * Registers a "cmod Build" entry in the run configuration dropdown,
 * allowing users to configure and execute build commands.
 */
class CmodBuildRunConfigurationType : ConfigurationType {

    companion object {
        const val ID = "CmodBuildRunConfiguration"
    }

    override fun getDisplayName(): String = "cmod Build"

    override fun getConfigurationTypeDescription(): String =
        "Build a cmod C++ module project"

    override fun getIcon(): Icon = AllIcons.Actions.Compile

    override fun getId(): String = ID

    override fun getConfigurationFactories(): Array<ConfigurationFactory> {
        return arrayOf(CmodBuildConfigurationFactory(this))
    }

    private class CmodBuildConfigurationFactory(type: ConfigurationType) : ConfigurationFactory(type) {

        override fun getId(): String = "CmodBuildConfigurationFactory"

        override fun getName(): String = "cmod Build"

        override fun createTemplateConfiguration(project: Project): RunConfiguration {
            return CmodBuildRunConfiguration(project, this, "cmod Build")
        }
    }
}
