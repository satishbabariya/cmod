package com.cmod.intellij.runConfig

import com.cmod.intellij.util.CmodProcessUtil
import com.intellij.execution.Executor
import com.intellij.execution.configurations.*
import com.intellij.execution.process.OSProcessHandler
import com.intellij.execution.runners.ExecutionEnvironment
import com.intellij.openapi.options.SettingsEditor
import com.intellij.openapi.project.Project
import org.jdom.Element

/**
 * Run configuration for `cmod run`.
 *
 * Configurable options:
 * - profile: "debug" (default) or "release"
 * - arguments: arguments to pass to the built binary (after --)
 */
class CmodRunConfiguration(
    project: Project,
    factory: ConfigurationFactory,
    name: String
) : RunConfigurationBase<RunProfileState>(project, factory, name) {

    var profile: String = "debug"
    var arguments: String = ""

    override fun getConfigurationEditor(): SettingsEditor<out RunConfiguration> {
        return CmodRunConfigEditor()
    }

    override fun getState(executor: Executor, environment: ExecutionEnvironment): RunProfileState {
        return object : CommandLineState(environment) {
            override fun startProcess(): OSProcessHandler {
                val args = buildCommandArgs()
                val handler = CmodProcessUtil.createProcessHandler(project, args)
                handler.addProcessListener(CmodProcessHandler())
                return handler
            }
        }
    }

    private fun buildCommandArgs(): List<String> {
        val args = mutableListOf("run")

        if (profile == "release") {
            args.add("--release")
        }

        if (arguments.isNotBlank()) {
            args.add("--")
            args.addAll(arguments.split("\\s+".toRegex()).filter { it.isNotBlank() })
        }

        return args
    }

    override fun readExternal(element: Element) {
        super.readExternal(element)
        profile = element.getAttributeValue("profile") ?: "debug"
        arguments = element.getAttributeValue("arguments") ?: ""
    }

    override fun writeExternal(element: Element) {
        super.writeExternal(element)
        element.setAttribute("profile", profile)
        element.setAttribute("arguments", arguments)
    }
}
