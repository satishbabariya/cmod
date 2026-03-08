package com.cmod.intellij.runConfig

import com.cmod.intellij.util.CmodProcessUtil
import com.intellij.execution.Executor
import com.intellij.execution.configurations.*
import com.intellij.execution.process.OSProcessHandler
import com.intellij.execution.process.ProcessHandlerFactory
import com.intellij.execution.runners.ExecutionEnvironment
import com.intellij.openapi.options.SettingsEditor
import com.intellij.openapi.project.Project
import org.jdom.Element

/**
 * Run configuration for `cmod build`.
 *
 * Configurable options:
 * - profile: "debug" (default) or "release"
 * - jobs: number of parallel compilation jobs (0 = auto)
 * - force: rebuild all modules even if cached
 * - timings: display per-module timing information
 */
class CmodBuildRunConfiguration(
    project: Project,
    factory: ConfigurationFactory,
    name: String
) : RunConfigurationBase<RunProfileState>(project, factory, name) {

    var profile: String = "debug"
    var jobs: Int = 0
    var force: Boolean = false
    var timings: Boolean = false

    override fun getConfigurationEditor(): SettingsEditor<out RunConfiguration> {
        return CmodBuildConfigEditor()
    }

    override fun getState(executor: Executor, environment: ExecutionEnvironment): RunProfileState {
        return object : CommandLineState(environment) {
            override fun startProcess(): OSProcessHandler {
                val args = buildCommandArgs()
                val commandLine = CmodProcessUtil.createProcessHandler(project, args)
                commandLine.addProcessListener(CmodProcessHandler())
                return commandLine
            }
        }
    }

    private fun buildCommandArgs(): List<String> {
        val args = mutableListOf("build")

        if (profile == "release") {
            args.add("--release")
        }

        if (jobs > 0) {
            args.add("--jobs")
            args.add(jobs.toString())
        }

        if (force) {
            args.add("--force")
        }

        if (timings) {
            args.add("--timings")
        }

        return args
    }

    override fun readExternal(element: Element) {
        super.readExternal(element)
        profile = element.getAttributeValue("profile") ?: "debug"
        jobs = element.getAttributeValue("jobs")?.toIntOrNull() ?: 0
        force = element.getAttributeValue("force")?.toBoolean() ?: false
        timings = element.getAttributeValue("timings")?.toBoolean() ?: false
    }

    override fun writeExternal(element: Element) {
        super.writeExternal(element)
        element.setAttribute("profile", profile)
        element.setAttribute("jobs", jobs.toString())
        element.setAttribute("force", force.toString())
        element.setAttribute("timings", timings.toString())
    }
}
