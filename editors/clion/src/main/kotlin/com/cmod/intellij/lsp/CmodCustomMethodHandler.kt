package com.cmod.intellij.lsp

import com.cmod.intellij.util.CmodProcessUtil
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.google.gson.JsonElement
import com.google.gson.JsonParser
import java.util.concurrent.CompletableFuture

/**
 * Handles custom LSP method requests by falling back to CLI invocations.
 *
 * When the LSP server does not support certain requests natively (e.g., module
 * graph visualization, cache status), this handler runs the corresponding cmod
 * CLI command and returns the result as JSON.
 */
class CmodCustomMethodHandler(private val project: Project) {

    companion object {
        private val LOG = Logger.getInstance(CmodCustomMethodHandler::class.java)

        const val METHOD_GRAPH = "cmod/graph"
        const val METHOD_CACHE_STATUS = "cmod/cacheStatus"
        const val METHOD_EXPLAIN = "cmod/explain"
        const val METHOD_DEPS = "cmod/deps"
    }

    /**
     * Dispatches a custom method request to the appropriate CLI command.
     * Returns a CompletableFuture with the parsed JSON response.
     */
    fun handleCustomMethod(method: String, params: JsonElement?): CompletableFuture<JsonElement?> {
        return CompletableFuture.supplyAsync {
            try {
                when (method) {
                    METHOD_GRAPH -> handleGraph()
                    METHOD_CACHE_STATUS -> handleCacheStatus()
                    METHOD_EXPLAIN -> handleExplain(params)
                    METHOD_DEPS -> handleDeps()
                    else -> {
                        LOG.warn("Unknown custom method: $method")
                        null
                    }
                }
            } catch (e: Exception) {
                LOG.error("Error handling custom method $method", e)
                null
            }
        }
    }

    /**
     * Runs `cmod graph --format json --status --timing` and returns the JSON output.
     */
    private fun handleGraph(): JsonElement? {
        val result = CmodProcessUtil.runCmodCommand(
            project,
            listOf("graph", "--format", "json", "--status", "--timing")
        )
        if (result.exitCode != 0) {
            LOG.warn("cmod graph failed with exit code ${result.exitCode}: ${result.stderr}")
            return null
        }
        return parseJson(result.stdout)
    }

    /**
     * Runs `cmod cache status` and returns the parsed JSON output.
     */
    private fun handleCacheStatus(): JsonElement? {
        val result = CmodProcessUtil.runCmodCommand(
            project,
            listOf("cache", "status", "--json")
        )
        if (result.exitCode != 0) {
            LOG.warn("cmod cache status failed with exit code ${result.exitCode}: ${result.stderr}")
            return null
        }
        return parseJson(result.stdout)
    }

    /**
     * Runs `cmod explain <module>` for the given module name from params.
     */
    private fun handleExplain(params: JsonElement?): JsonElement? {
        val moduleName = params?.asJsonObject?.get("module")?.asString
        if (moduleName.isNullOrBlank()) {
            LOG.warn("cmod/explain requires a 'module' parameter")
            return null
        }

        val result = CmodProcessUtil.runCmodCommand(
            project,
            listOf("explain", moduleName)
        )
        if (result.exitCode != 0) {
            LOG.warn("cmod explain failed with exit code ${result.exitCode}: ${result.stderr}")
            return null
        }
        return parseJson(result.stdout)
    }

    /**
     * Runs `cmod deps --tree` and returns the output.
     */
    private fun handleDeps(): JsonElement? {
        val result = CmodProcessUtil.runCmodCommand(
            project,
            listOf("deps", "--tree", "--json")
        )
        if (result.exitCode != 0) {
            LOG.warn("cmod deps failed with exit code ${result.exitCode}: ${result.stderr}")
            return null
        }
        return parseJson(result.stdout)
    }

    private fun parseJson(text: String): JsonElement? {
        return try {
            JsonParser.parseString(text)
        } catch (e: Exception) {
            LOG.warn("Failed to parse JSON response: ${e.message}")
            null
        }
    }
}
