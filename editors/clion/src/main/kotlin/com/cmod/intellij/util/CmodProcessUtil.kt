package com.cmod.intellij.util

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.execution.process.OSProcessHandler
import com.intellij.execution.process.ProcessAdapter
import com.intellij.execution.process.ProcessEvent
import com.intellij.execution.process.ProcessOutputTypes
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.Key
import java.util.concurrent.TimeUnit
import java.util.regex.Pattern

/**
 * Utility for running cmod CLI commands and parsing their output.
 *
 * Provides both synchronous and asynchronous execution, with support for
 * Clang diagnostic parsing.
 */
object CmodProcessUtil {

    private val LOG = Logger.getInstance(CmodProcessUtil::class.java)

    /**
     * Result of a cmod command execution.
     */
    data class CommandResult(
        val exitCode: Int,
        val stdout: String,
        val stderr: String,
    ) {
        val isSuccess: Boolean get() = exitCode == 0
    }

    /**
     * Parsed Clang diagnostic from build output.
     *
     * Matches the format: file:line:col: severity: message
     */
    data class ClangDiagnostic(
        val file: String,
        val line: Int,
        val column: Int,
        val severity: String, // "error", "warning", "note"
        val message: String,
    )

    /** Regex for parsing Clang diagnostic output. */
    private val CLANG_DIAG_PATTERN = Pattern.compile(
        """^(.+?):(\d+):(\d+):\s+(error|warning|note|fatal error):\s+(.+)$"""
    )

    /**
     * Runs a cmod command synchronously and returns the result.
     *
     * @param project The current project (used for working directory).
     * @param args The cmod subcommand and arguments (e.g., ["build", "--release"]).
     * @param timeoutSeconds Maximum time to wait for the command to complete.
     * @return CommandResult with exit code, stdout, and stderr.
     */
    fun runCmodCommand(
        project: Project,
        args: List<String>,
        timeoutSeconds: Long = 120
    ): CommandResult {
        val cmodBinary = CmodBinaryUtil.getCmodBinaryPath()
        val commandLine = GeneralCommandLine(cmodBinary)
            .withParameters(args)
            .withWorkDirectory(project.basePath)
            .withCharset(Charsets.UTF_8)
            .withRedirectErrorStream(false)

        LOG.info("Running: $cmodBinary ${args.joinToString(" ")}")

        return try {
            val process = commandLine.createProcess()
            val stdout = process.inputStream.bufferedReader().readText()
            val stderr = process.errorStream.bufferedReader().readText()
            val completed = process.waitFor(timeoutSeconds, TimeUnit.SECONDS)

            if (!completed) {
                process.destroyForcibly()
                LOG.warn("cmod command timed out after ${timeoutSeconds}s: ${args.joinToString(" ")}")
                CommandResult(-1, stdout, "Command timed out after ${timeoutSeconds}s")
            } else {
                CommandResult(process.exitValue(), stdout, stderr)
            }
        } catch (e: Exception) {
            LOG.error("Failed to run cmod command: ${args.joinToString(" ")}", e)
            CommandResult(-1, "", e.message ?: "Unknown error")
        }
    }

    /**
     * Creates an OSProcessHandler for a cmod command, suitable for integration
     * with IntelliJ run configurations and console views.
     *
     * @param project The current project.
     * @param args The cmod subcommand and arguments.
     * @return An OSProcessHandler for the command.
     */
    fun createProcessHandler(project: Project, args: List<String>): OSProcessHandler {
        val cmodBinary = CmodBinaryUtil.getCmodBinaryPath()
        val commandLine = GeneralCommandLine(cmodBinary)
            .withParameters(args)
            .withWorkDirectory(project.basePath)
            .withCharset(Charsets.UTF_8)

        return OSProcessHandler(commandLine)
    }

    /**
     * Parses Clang diagnostic lines from build output text.
     *
     * Recognizes the standard Clang format: file:line:col: severity: message
     *
     * @param output The raw build output text.
     * @return A list of parsed ClangDiagnostic objects.
     */
    fun parseClangDiagnostics(output: String): List<ClangDiagnostic> {
        val diagnostics = mutableListOf<ClangDiagnostic>()

        for (line in output.lines()) {
            val matcher = CLANG_DIAG_PATTERN.matcher(line)
            if (matcher.matches()) {
                diagnostics.add(
                    ClangDiagnostic(
                        file = matcher.group(1),
                        line = matcher.group(2).toIntOrNull() ?: 0,
                        column = matcher.group(3).toIntOrNull() ?: 0,
                        severity = matcher.group(4),
                        message = matcher.group(5),
                    )
                )
            }
        }

        return diagnostics
    }

    /**
     * Creates a ProcessAdapter that collects output and parses diagnostics
     * upon process termination.
     */
    fun createDiagnosticCollector(
        onDiagnostics: (List<ClangDiagnostic>) -> Unit
    ): ProcessAdapter {
        return object : ProcessAdapter() {
            private val outputBuffer = StringBuilder()
            private val errorBuffer = StringBuilder()

            override fun onTextAvailable(event: ProcessEvent, outputType: Key<*>) {
                when (outputType) {
                    ProcessOutputTypes.STDOUT -> outputBuffer.append(event.text)
                    ProcessOutputTypes.STDERR -> errorBuffer.append(event.text)
                }
            }

            override fun processTerminated(event: ProcessEvent) {
                val allOutput = outputBuffer.toString() + errorBuffer.toString()
                val diagnostics = parseClangDiagnostics(allOutput)
                if (diagnostics.isNotEmpty()) {
                    onDiagnostics(diagnostics)
                }
            }
        }
    }
}
