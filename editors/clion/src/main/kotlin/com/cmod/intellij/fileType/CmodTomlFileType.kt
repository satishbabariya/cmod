package com.cmod.intellij.fileType

import com.intellij.openapi.fileTypes.FileType
import javax.swing.Icon

/**
 * File type for cmod.toml manifest files.
 *
 * The cmod.toml file is the project manifest that defines package metadata,
 * dependencies, build settings, and workspace configuration.
 */
class CmodTomlFileType private constructor() : FileType {

    companion object {
        @JvmStatic
        val INSTANCE = CmodTomlFileType()
    }

    override fun getName(): String = "cmod Manifest"

    override fun getDescription(): String = "cmod project manifest (cmod.toml)"

    override fun getDefaultExtension(): String = "toml"

    override fun getIcon(): Icon? = null

    override fun isBinary(): Boolean = false

    override fun isReadOnly(): Boolean = false
}
