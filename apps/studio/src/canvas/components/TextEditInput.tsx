import type { useTextEdit } from "../hooks/use-text-edit"

type TextEditInputController = Pick<
  ReturnType<typeof useTextEdit>,
  | "handleBlur"
  | "handleCompositionEnd"
  | "handleCompositionStart"
  | "handleCompositionUpdate"
  | "handleInput"
  | "handleKeyDown"
  | "handleKeyUp"
  | "handleSelect"
  | "inputRef"
  | "isEditing"
>

export function TextEditInput({
  textEdit,
}: {
  textEdit: TextEditInputController
}) {
  return (
    <textarea
      onBlur={textEdit.handleBlur}
      onCompositionEnd={textEdit.handleCompositionEnd}
      onCompositionStart={textEdit.handleCompositionStart}
      onCompositionUpdate={textEdit.handleCompositionUpdate}
      onInput={textEdit.handleInput}
      onKeyDown={textEdit.handleKeyDown}
      onKeyUp={textEdit.handleKeyUp}
      onSelect={textEdit.handleSelect}
      ref={textEdit.inputRef}
      style={{
        position: "fixed",
        pointerEvents: "none",
        zIndex: 10_000,
        height: "1px",
        border: "none",
        outline: "none",
        padding: 0,
        margin: 0,
        opacity: textEdit.isEditing ? 1 : 0,
        background: "transparent",
        color: "CanvasText",
        caretColor: "auto",
        overflow: "hidden",
        resize: "none",
        whiteSpace: "pre-wrap",
        visibility: textEdit.isEditing ? "visible" : "hidden",
      }}
    />
  )
}
