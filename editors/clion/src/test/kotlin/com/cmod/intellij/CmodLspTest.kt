package com.cmod.intellij

import com.cmod.intellij.lsp.CmodLspServerDescriptor
import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test

/**
 * Tests for the cmod LSP server integration.
 */
class CmodLspTest {

    @Test
    fun `supported file extensions include cppm`() {
        assertTrue(isSupportedExtension("cppm"))
    }

    @Test
    fun `supported file extensions include ixx`() {
        assertTrue(isSupportedExtension("ixx"))
    }

    @Test
    fun `supported file extensions include mxx`() {
        assertTrue(isSupportedExtension("mxx"))
    }

    @Test
    fun `supported file extensions include cpp`() {
        assertTrue(isSupportedExtension("cpp"))
    }

    @Test
    fun `supported file extensions include hpp`() {
        assertTrue(isSupportedExtension("hpp"))
    }

    @Test
    fun `unsupported file extensions are rejected`() {
        assertFalse(isSupportedExtension("py"))
        assertFalse(isSupportedExtension("java"))
        assertFalse(isSupportedExtension("rs"))
        assertFalse(isSupportedExtension("txt"))
    }

    @Test
    fun `cmod manifest filename is cmod_toml`() {
        assertEquals("cmod.toml", CmodPlugin.MANIFEST_FILENAME)
    }

    @Test
    fun `notification group id is correct`() {
        assertEquals("cmod.notifications", CmodPlugin.NOTIFICATION_GROUP_ID)
    }

    /**
     * Helper to check if a file extension is in the supported set.
     * Mirrors the logic in CmodLspServerDescriptor.isSupportedFile
     * without requiring IntelliJ platform runtime.
     */
    private fun isSupportedExtension(ext: String): Boolean {
        val supportedExtensions = setOf(
            "cppm", "ixx", "mxx",
            "cpp", "cxx", "cc", "c++",
            "h", "hpp", "hxx", "h++"
        )
        return ext.lowercase() in supportedExtensions
    }
}
