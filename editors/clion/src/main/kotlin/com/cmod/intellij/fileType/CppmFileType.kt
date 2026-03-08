package com.cmod.intellij.fileType

import com.intellij.openapi.fileTypes.FileType
import javax.swing.Icon

/**
 * File type for C++20 module interface files (.cppm, .ixx, .mxx).
 *
 * These files contain C++20 module declarations and are the primary
 * compilation units for cmod projects.
 */
class CppmFileType private constructor() : FileType {

    companion object {
        @JvmStatic
        val INSTANCE = CppmFileType()
    }

    override fun getName(): String = "C++ Module Interface"

    override fun getDescription(): String = "C++20 module interface file"

    override fun getDefaultExtension(): String = "cppm"

    override fun getIcon(): Icon? = null

    override fun isBinary(): Boolean = false

    override fun isReadOnly(): Boolean = false
}
