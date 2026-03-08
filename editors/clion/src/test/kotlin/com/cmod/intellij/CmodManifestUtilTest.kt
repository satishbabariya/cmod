package com.cmod.intellij

import com.cmod.intellij.util.CmodManifestUtil
import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.nio.file.Path

/**
 * Tests for cmod.toml manifest parsing utility.
 */
class CmodManifestUtilTest {

    @TempDir
    lateinit var tempDir: Path

    @Test
    fun `parse minimal manifest`() {
        val manifest = createManifest("""
            [package]
            name = "hello"
            version = "0.1.0"
        """.trimIndent())

        assertNotNull(manifest)
        assertEquals("hello", manifest!!.name)
        assertEquals("0.1.0", manifest.version)
        assertTrue(manifest.dependencies.isEmpty())
    }

    @Test
    fun `parse manifest with dependencies`() {
        val manifest = createManifest("""
            [package]
            name = "myproject"
            version = "1.0.0"
            edition = "2023"

            [module]
            name = "com.github.user.myproject"

            [dependencies]
            fmt = ">=10.0.0"
            json = ">=3.11.0"
        """.trimIndent())

        assertNotNull(manifest)
        assertEquals("myproject", manifest!!.name)
        assertEquals("1.0.0", manifest.version)
        assertEquals("2023", manifest.edition)
        assertEquals("com.github.user.myproject", manifest.moduleName)
        assertEquals(2, manifest.dependencies.size)
        assertEquals(">=10.0.0", manifest.dependencies["fmt"])
        assertEquals(">=3.11.0", manifest.dependencies["json"])
    }

    @Test
    fun `parse manifest with build section`() {
        val manifest = createManifest("""
            [package]
            name = "mylib"
            version = "0.1.0"

            [build]
            type = "library"
        """.trimIndent())

        assertNotNull(manifest)
        assertEquals("library", manifest!!.buildType)
    }

    @Test
    fun `parse manifest with workspace members`() {
        val manifest = createManifest("""
            [workspace]
            members = ["core", "cli", "utils"]
        """.trimIndent())

        assertNotNull(manifest)
        assertEquals(3, manifest!!.workspaceMembers.size)
        assertTrue("core" in manifest.workspaceMembers)
        assertTrue("cli" in manifest.workspaceMembers)
        assertTrue("utils" in manifest.workspaceMembers)
    }

    @Test
    fun `parse manifest with comments`() {
        val manifest = createManifest("""
            # This is a comment
            [package]
            name = "hello"
            # Another comment
            version = "0.1.0"
        """.trimIndent())

        assertNotNull(manifest)
        assertEquals("hello", manifest!!.name)
        assertEquals("0.1.0", manifest.version)
    }

    @Test
    fun `parse manifest with single-quoted values`() {
        val manifest = createManifest("""
            [package]
            name = 'single-quoted'
            version = '1.0.0'
        """.trimIndent())

        assertNotNull(manifest)
        assertEquals("single-quoted", manifest!!.name)
    }

    @Test
    fun `parse empty manifest`() {
        val manifest = createManifest("")
        assertNotNull(manifest)
        assertEquals("", manifest!!.name)
        assertTrue(manifest.dependencies.isEmpty())
    }

    @Test
    fun `parse nonexistent file returns null`() {
        val file = File(tempDir.toFile(), "nonexistent.toml")
        val manifest = CmodManifestUtil.parseManifestFile(file)
        assertNull(manifest)
    }

    @Test
    fun `parse full manifest`() {
        val manifest = createManifest("""
            [package]
            name = "with-deps"
            version = "0.1.0"
            edition = "2023"

            [module]
            name = "com.github.user.with_deps"

            [build]
            type = "binary"

            [dependencies]
            fmt = ">=10.0.0"
            json = ">=3.11.0"
            spdlog = ">=1.12.0"

            [workspace]
            members = ["core", "app"]
        """.trimIndent())

        assertNotNull(manifest)
        assertEquals("with-deps", manifest!!.name)
        assertEquals("0.1.0", manifest.version)
        assertEquals("2023", manifest.edition)
        assertEquals("com.github.user.with_deps", manifest.moduleName)
        assertEquals("binary", manifest.buildType)
        assertEquals(3, manifest.dependencies.size)
        assertEquals(2, manifest.workspaceMembers.size)
    }

    private fun createManifest(content: String): CmodManifestUtil.ManifestInfo? {
        val file = File(tempDir.toFile(), "cmod.toml")
        file.writeText(content)
        return CmodManifestUtil.parseManifestFile(file)
    }
}
