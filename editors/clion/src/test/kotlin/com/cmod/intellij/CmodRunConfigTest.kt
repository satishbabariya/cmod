package com.cmod.intellij

import com.cmod.intellij.util.CmodProcessUtil
import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test

/**
 * Tests for cmod run configuration logic and diagnostic parsing.
 */
class CmodRunConfigTest {

    @Test
    fun `parse single clang error diagnostic`() {
        val output = "/home/user/project/src/main.cpp:10:5: error: use of undeclared identifier 'foo'"
        val diagnostics = CmodProcessUtil.parseClangDiagnostics(output)

        assertEquals(1, diagnostics.size)
        val diag = diagnostics[0]
        assertEquals("/home/user/project/src/main.cpp", diag.file)
        assertEquals(10, diag.line)
        assertEquals(5, diag.column)
        assertEquals("error", diag.severity)
        assertEquals("use of undeclared identifier 'foo'", diag.message)
    }

    @Test
    fun `parse single clang warning diagnostic`() {
        val output = "src/lib.cppm:42:12: warning: unused variable 'x' [-Wunused-variable]"
        val diagnostics = CmodProcessUtil.parseClangDiagnostics(output)

        assertEquals(1, diagnostics.size)
        val diag = diagnostics[0]
        assertEquals("src/lib.cppm", diag.file)
        assertEquals(42, diag.line)
        assertEquals(12, diag.column)
        assertEquals("warning", diag.severity)
        assertEquals("unused variable 'x' [-Wunused-variable]", diag.message)
    }

    @Test
    fun `parse fatal error diagnostic`() {
        val output = "src/main.cpp:1:10: fatal error: 'nonexistent.h' file not found"
        val diagnostics = CmodProcessUtil.parseClangDiagnostics(output)

        assertEquals(1, diagnostics.size)
        assertEquals("fatal error", diagnostics[0].severity)
    }

    @Test
    fun `parse note diagnostic`() {
        val output = "src/main.cpp:5:3: note: in instantiation of function template specialization"
        val diagnostics = CmodProcessUtil.parseClangDiagnostics(output)

        assertEquals(1, diagnostics.size)
        assertEquals("note", diagnostics[0].severity)
    }

    @Test
    fun `parse multiple diagnostics`() {
        val output = """
            src/main.cpp:10:5: error: use of undeclared identifier 'foo'
            src/main.cpp:10:5: note: did you mean 'bar'?
            src/lib.cppm:42:12: warning: unused variable 'x' [-Wunused-variable]
            Some other output line that should be ignored
            src/main.cpp:20:1: error: expected ';' after expression
        """.trimIndent()

        val diagnostics = CmodProcessUtil.parseClangDiagnostics(output)

        assertEquals(4, diagnostics.size)
        assertEquals("error", diagnostics[0].severity)
        assertEquals("note", diagnostics[1].severity)
        assertEquals("warning", diagnostics[2].severity)
        assertEquals("error", diagnostics[3].severity)
    }

    @Test
    fun `parse empty output returns no diagnostics`() {
        val diagnostics = CmodProcessUtil.parseClangDiagnostics("")
        assertTrue(diagnostics.isEmpty())
    }

    @Test
    fun `parse non-diagnostic output returns no diagnostics`() {
        val output = """
            Building module graph...
            Compiling src/main.cpp
            Linking myapp
            Build completed in 1.23s
        """.trimIndent()

        val diagnostics = CmodProcessUtil.parseClangDiagnostics(output)
        assertTrue(diagnostics.isEmpty())
    }

    @Test
    fun `command result isSuccess for exit code 0`() {
        val result = CmodProcessUtil.CommandResult(0, "output", "")
        assertTrue(result.isSuccess)
    }

    @Test
    fun `command result isSuccess false for non-zero exit code`() {
        val result = CmodProcessUtil.CommandResult(1, "", "error")
        assertFalse(result.isSuccess)
    }

    @Test
    fun `parse windows-style paths in diagnostics`() {
        val output = """C:\Users\dev\project\src\main.cpp:10:5: error: some error"""
        val diagnostics = CmodProcessUtil.parseClangDiagnostics(output)

        assertEquals(1, diagnostics.size)
        assertEquals("""C:\Users\dev\project\src\main.cpp""", diagnostics[0].file)
    }
}
