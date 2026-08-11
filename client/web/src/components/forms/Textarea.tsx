import { useId, useState } from "react";
import type { CSSProperties, TextareaHTMLAttributes } from "react";

/**
 * Multi-line prose, styled exactly as `Input` is — same label, hint/error
 * association, border, focus ring and tokens.
 *
 * **An addition to the design system, not a mirror of it**: the 16-component
 * library (`.claude/skills/hummingbird-design/`) has no textarea, because no
 * shipped screen needed one until an item's `description` — the schema's only
 * free-prose field (ADR-0009) — became editable. Worth raising upstream in the
 * design project; until it exists there, this is the local answer, and it
 * deliberately copies `Input`'s structure rather than inventing a second
 * treatment for the same kind of control.
 */
export interface TextareaProps
  extends Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "style"> {
  label?: string;
  /** Helper line under the field; replaced by `error` when set. */
  hint?: string;
  error?: string;
  style?: CSSProperties;
}

export function Textarea({ label, hint, error, id, rows = 3, style = {}, ...rest }: TextareaProps) {
  const [focus, setFocus] = useState(false);
  const autoId = useId();
  const fieldId = id || autoId;
  const describedById = fieldId + "-desc";
  const describedBy = error || hint ? describedById : undefined;
  const borderColor = error
    ? "var(--status-danger-fg)"
    : focus
      ? "var(--accent)"
      : "var(--border-default)";
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)", ...style }}>
      {label ? (
        <label
          htmlFor={fieldId}
          style={{
            font: "var(--weight-semibold) var(--size-body-sm)/1.2 var(--font-sans)",
            color: "var(--text-secondary)",
          }}
        >
          {label}
        </label>
      ) : null}
      <textarea
        id={fieldId}
        rows={rows}
        aria-describedby={describedBy}
        aria-invalid={error ? true : undefined}
        {...rest}
        onFocus={(event) => {
          setFocus(true);
          rest.onFocus?.(event);
        }}
        onBlur={(event) => {
          setFocus(false);
          rest.onBlur?.(event);
        }}
        // One element, so the ring belongs to it — unlike `Input`, whose
        // bordered wrapper owns the ring and whose inner element has to
        // suppress the global `:focus-visible` one to avoid drawing two.
        style={{
          width: "100%",
          padding: "var(--space-4) var(--space-5)",
          background: "var(--surface-card)",
          border: `1px solid ${borderColor}`,
          borderRadius: "var(--radius-control)",
          boxShadow: focus ? "var(--ring-focus)" : "none",
          outline: "none",
          font: "var(--type-body)",
          color: "var(--text-primary)",
          resize: "vertical",
          transition:
            "border-color var(--dur-fast) var(--ease-flit), box-shadow var(--dur-fast) var(--ease-flit)",
        }}
      />
      {error ? (
        <span
          id={describedById}
          style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)" }}
        >
          {error}
        </span>
      ) : hint ? (
        <span id={describedById} style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>
          {hint}
        </span>
      ) : null}
    </div>
  );
}
