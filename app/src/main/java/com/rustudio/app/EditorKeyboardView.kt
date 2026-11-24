package com.rustudio.app

import android.content.Context
import android.text.InputType
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection

/** Gives the IME an input connection while egui owns the document and cursor. */
internal class EditorKeyboardView(
    context: Context,
    private val send: (generation: Int, kind: Int, text: String, before: Int, after: Int) -> Unit,
) : View(context) {
    private var mode = 0
    private var generation = 0

    init {
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO
    }

    fun matches(mode: Int, generation: Int) = this.mode == mode && this.generation == generation

    fun updateTarget(mode: Int, generation: Int): Boolean {
        val changed = !matches(mode, generation)
        this.mode = mode
        this.generation = generation
        return changed
    }

    override fun onCheckIsTextEditor() = mode != 0

    override fun onCreateInputConnection(info: EditorInfo): InputConnection? {
        if (mode == 0) return null
        info.inputType = InputType.TYPE_CLASS_TEXT or if (mode == 2) {
            InputType.TYPE_TEXT_VARIATION_PASSWORD
        } else {
            InputType.TYPE_TEXT_FLAG_MULTI_LINE
        }
        info.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI or
            EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING or EditorInfo.IME_FLAG_NO_ENTER_ACTION
        info.initialSelStart = 0
        info.initialSelEnd = 0
        val target = generation
        return object : BaseInputConnection(this, true) {
            private var composing = ""

            override fun setComposingText(text: CharSequence, newCursorPosition: Int): Boolean {
                composing = text.toString()
                send(target, PREEDIT, composing, 0, 0)
                return true
            }

            override fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {
                // Enter needs to follow the same path as a physical key so
                // single-line fields submit and the script editor inserts a newline.
                if (text.length == 1 && (text[0] == '\n' || text[0] == '\r')) {
                    return performEditorAction(EditorInfo.IME_ACTION_NONE)
                }
                composing = ""
                send(target, COMMIT, text.toString(), 0, 0)
                return true
            }

            override fun finishComposingText(): Boolean {
                if (composing.isNotEmpty()) {
                    send(target, COMMIT, composing, 0, 0)
                    composing = ""
                }
                return true
            }

            override fun getTextBeforeCursor(length: Int, flags: Int): CharSequence =
                composing.takeLast(length.coerceAtLeast(0))

            override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
                finishComposingText()
                send(target, DELETE, "", beforeLength, afterLength)
                return true
            }

            override fun deleteSurroundingTextInCodePoints(beforeLength: Int, afterLength: Int) =
                deleteSurroundingText(beforeLength, afterLength)

            override fun performEditorAction(actionCode: Int): Boolean {
                finishComposingText()
                send(target, KEY, "", KeyEvent.KEYCODE_ENTER, 1)
                send(target, KEY, "", KeyEvent.KEYCODE_ENTER, 0)
                return true
            }

            override fun sendKeyEvent(event: KeyEvent): Boolean {
                return when (event.keyCode) {
                    KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_DEL, KeyEvent.KEYCODE_FORWARD_DEL,
                    KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_DOWN,
                    KeyEvent.KEYCODE_DPAD_LEFT, KeyEvent.KEYCODE_DPAD_RIGHT, KeyEvent.KEYCODE_TAB -> {
                        finishComposingText()
                        send(target, KEY, "", event.keyCode, if (event.action == KeyEvent.ACTION_DOWN) 1 else 0)
                        true
                    }
                    else -> super.sendKeyEvent(event)
                }
            }
        }
    }

    private companion object {
        const val PREEDIT = 0
        const val COMMIT = 1
        const val DELETE = 2
        const val KEY = 3
    }
}
