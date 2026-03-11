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
 * Run configuration for `cmod test`.
 *
 * Configurable options:
 * - filter: test name filter pattern (empty = run all)
 * - profile: "debug" (default) or "release"
 * - coverage: enable code coverage collection
 * - sanitizer: optional sanitizer (none, address, thread, undefined, memory)
 */
class CmodTestRunConfiguration(
    project: Project,
    factory: ConfigurationFactory,
    name: String
) : RunConfigurationBase<RunProfileState>(project, factory, name) {

    var filter: String = ""
    var profile: String = "debug"
    var coverage: Boolean = false
    var sanitizer: String = "none"

    override fun getConfigurationEditor(): SettingsEditor<out RunConfiguration> {
        return CmodTestConfigEditor()
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
        val args = mutableListOf("test")

        if (profile == "release") {
            args.add("--release")
        }

        if (filter.isNotBlank()) {
            args.add("--filter")
            args.add(filter)
        }

        if (coverage) {
            args.add("--coverage")
        }

        if (sanitizer != "none" && sanitizer.isNotBlank()) {
            args.add("--sanitize")
            args.add(sanitizer)
        }

        return args
    }

    override fun readExternal(element: Element) {
        super.readExternal(element)
        filter = element.getAttributeValue("filter") ?: ""
        profile = element.getAttributeValue("profile") ?: "debug"
        coverage = element.getAttributeValue("coverage")?.toBoolean() ?: false
        sanitizer = element.getAttributeValue("sanitizer") ?: "none"
    }

    override fun writeExternal(element: Element) {
        super.writeExternal(element)
        element.setAttribute("filter", filter)
        element.setAttribute("profile", profile)
        element.setAttribute("coverage", coverage.toString())
        element.setAttribute("sanitizer", sanitizer)
    }
}
