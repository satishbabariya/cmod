package com.cmod.intellij.runConfig

import com.intellij.execution.process.ProcessAdapter
import com.intellij.execution.process.ProcessEvent
import com.intellij.execution.process.ProcessOutputTypes
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.util.Key
import java.util.regex.Pattern

/**
 * Shared process listener for cmod run configurations.
 *
 * Parses Clang diagnostic output in real-time from stdout/stderr and logs
 * structured diagnostic information. Recognizes the Clang diagnostic format:
 *   file:line:col: severity: message
 *
 * This handler is attached to all cmod build/test/run process handlers
 * to provide consistent diagnostic parsing.
 */
class CmodProcessHandler : ProcessAdapter() {

    companion object {
        private val LOG = Logger.getInstance(CmodProcessHandler::class.java)

        /** Pattern matching Clang diagnostic format: file:line:col: severity: message */
        private val DIAGNOSTIC_PATTERN = Pattern.compile(
            """^(.+?):(\d+):(\d+):\s+(error|warning|note|fatal error):\s+(.+)$"""
        )

        /** Pattern matching linker errors */
        private val LINKER_ERROR_PATTERN = Pattern.compile(
            """^(?:ld|lld|link).*?:\s+error:\s+(.+)$"""
        )
    }

    private var errorCount = 0
    private var warningCount = 0

    override fun onTextAvailable(event: ProcessEvent, outputType: Key<*>) {
        val text = event.text.trim()
        if (text.isEmpty()) return

        if (outputType == ProcessOutputTypes.STDERR || outputType == ProcessOutputTypes.STDOUT) {
            parseDiagnosticLine(text)
        }
    }

    override fun processTerminated(event: ProcessEvent) {
        val exitCode = event.exitCode
        if (exitCode == 0) {
            LOG.info("cmod process completed successfully")
        } else {
            LOG.info("cmod process exited with code $exitCode ($errorCount errors, $warningCount warnings)")
        }
    }

    private fun parseDiagnosticLine(line: String) {
        // Check for Clang diagnostics
        val diagMatcher = DIAGNOSTIC_PATTERN.matcher(line)
        if (diagMatcher.matches()) {
            val file = diagMatcher.group(1)
            val lineNum = diagMatcher.group(2)
            val col = diagMatcher.group(3)
            val severity = diagMatcher.group(4)
            val message = diagMatcher.group(5)

            when (severity) {
                "error", "fatal error" -> {
                    errorCount++
                    LOG.warn("[$severity] $file:$lineNum:$col: $message")
                }
                "warning" -> {
                    warningCount++
                    LOG.info("[warning] $file:$lineNum:$col: $message")
                }
                "note" -> {
                    LOG.info("[note] $file:$lineNum:$col: $message")
                }
            }
            return
        }

        // Check for linker errors
        val linkerMatcher = LINKER_ERROR_PATTERN.matcher(line)
        if (linkerMatcher.matches()) {
            errorCount++
            LOG.warn("[linker error] ${linkerMatcher.group(1)}")
        }
    }
}
