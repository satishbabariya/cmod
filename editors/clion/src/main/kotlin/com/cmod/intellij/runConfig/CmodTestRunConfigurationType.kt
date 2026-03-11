package com.cmod.intellij.runConfig

import com.intellij.execution.configurations.ConfigurationFactory
import com.intellij.execution.configurations.ConfigurationType
import com.intellij.execution.configurations.RunConfiguration
import com.intellij.icons.AllIcons
import com.intellij.openapi.project.Project
import javax.swing.Icon

/**
 * Run configuration type for `cmod test`.
 *
 * Registers a "cmod Test" entry in the run configuration dropdown,
 * allowing users to configure and execute test commands.
 */
class CmodTestRunConfigurationType : ConfigurationType {

    companion object {
        const val ID = "CmodTestRunConfiguration"
    }

    override fun getDisplayName(): String = "cmod Test"

    override fun getConfigurationTypeDescription(): String =
        "Run tests for a cmod C++ module project"

    override fun getIcon(): Icon = AllIcons.RunConfigurations.TestState.Run

    override fun getId(): String = ID

    override fun getConfigurationFactories(): Array<ConfigurationFactory> {
        return arrayOf(CmodTestConfigurationFactory(this))
    }

    private class CmodTestConfigurationFactory(type: ConfigurationType) : ConfigurationFactory(type) {

        override fun getId(): String = "CmodTestConfigurationFactory"

        override fun getName(): String = "cmod Test"

        override fun createTemplateConfiguration(project: Project): RunConfiguration {
            return CmodTestRunConfiguration(project, this, "cmod Test")
        }
    }
}
