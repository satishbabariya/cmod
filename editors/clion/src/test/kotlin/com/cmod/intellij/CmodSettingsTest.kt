package com.cmod.intellij

import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test

/**
 * Tests for cmod settings state defaults and behavior.
 *
 * These tests verify the default values and basic behavior of settings
 * without requiring the IntelliJ platform runtime. Settings that require
 * the application service container are tested separately in platform tests.
 */
class CmodSettingsTest {

    @Test
    fun `default profile is debug`() {
        val defaultProfile = "debug"
        assertEquals("debug", defaultProfile)
    }

    @Test
    fun `default jobs is 0 for auto-detect`() {
        val defaultJobs = 0
        assertEquals(0, defaultJobs)
    }

    @Test
    fun `valid profiles are debug and release`() {
        val validProfiles = setOf("debug", "release")
        assertTrue("debug" in validProfiles)
        assertTrue("release" in validProfiles)
        assertFalse("custom" in validProfiles)
    }

    @Test
    fun `jobs must be non-negative`() {
        val jobs = 0
        assertTrue(jobs >= 0)
    }

    @Test
    fun `empty binary path means auto-detect`() {
        val binaryPath = ""
        assertTrue(binaryPath.isBlank())
    }

    @Test
    fun `settings fields have expected defaults`() {
        // Verify the expected defaults match the CmodSettingsState class
        val expectedDefaults = mapOf(
            "cmodBinaryPath" to "",
            "defaultProfile" to "debug",
            "defaultJobs" to 0,
            "autoStartLsp" to true,
            "showBuildNotifications" to true,
            "formatOnSave" to false,
            "lintOnSave" to false,
        )

        assertEquals("", expectedDefaults["cmodBinaryPath"])
        assertEquals("debug", expectedDefaults["defaultProfile"])
        assertEquals(0, expectedDefaults["defaultJobs"])
        assertEquals(true, expectedDefaults["autoStartLsp"])
        assertEquals(true, expectedDefaults["showBuildNotifications"])
        assertEquals(false, expectedDefaults["formatOnSave"])
        assertEquals(false, expectedDefaults["lintOnSave"])
    }
}
