import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties, KeyboardEvent as ReactKeyboardEvent } from "react";
import { Icon } from "../core/Icon";
import { Input } from "./Input";
import { comboboxOpenSignal } from "./combobox-open";
import { visibleSuggestions } from "./combobox-options";
import { popupPlacement, type PopupPlacement } from "./combobox-placement";

// A text field with suggestions: anything is typeable, and a named few are one
// click away. The form `Select` cannot do this — a `<select>`'s values *are*
// its vocabulary — so a field whose vocabulary is open in the schema has to be
// an input, or the client silently closes it (see `screens/field-vocabulary.ts`
// for the decision this exists to serve).
//
// A listbox of our own rather than a native `<datalist>`, which is what this
// was until #641 made the choice untenable. The native element buys
// filter-as-you-type, keyboard traversal, the mobile picker and the
// accessibility contract for free — a real saving, and the reason it was
// picked. What it does not buy, and cannot be talked out of, is a way to see
// the *whole* list: Chromium filters `<datalist>` options case-insensitively
// by substring against whatever the input currently holds. That was fine
// while the field was usually empty. #641 made capture's Context sticky
// across submits, so the box almost always holds a value, the popup almost
// always collapsed to the one option matching it, and a context minted since
// — `@shopping` — was unreachable without first clearing the field, with no
// affordance saying so. The rest of the vocabulary being invisible is worse
// than reimplementing traversal.
//
// So the chevron is now a real button, and it opens in `browseAll` mode:
// the whole vocabulary, whatever the box holds. `combobox-options.ts` is
// that rule, alone and unit-tested. Everything else here is markup, event
// plumbing and the ARIA contract `input[list]` used to give away — the
// `role="combobox"` / `aria-expanded` / `aria-activedescendant` triple has
// to be written by hand once the native element is gone.
//
// Escape is deliberately NOT bound here. The shell owns that key for every
// overlay at once (`shell/escape-claimants.ts`), and an open list inside the
// capture popover is exactly the case a second listener gets wrong; the open
// state is published through `combobox-open.ts` and closed from there.

/** `--space-2` of gap, `--space-4` clear of the viewport edge, and a popup
 * that stops at seven-ish options rather than running the height of the
 * screen. Pixels rather than token strings because they are arithmetic here,
 * not declarations. */
const POPUP_METRICS = { gap: 4, margin: 8, preferredHeight: 240 };

export interface ComboboxProps {
  label?: string;
  /** Values offered in the popup. Suggestions, never a constraint — a typed
   * value outside this list is as valid as one in it. */
  suggestions: readonly string[];
  value: string;
  onChange: (value: string) => void;
  /** Placeholder, and the honest place to say the field may be left empty. */
  placeholder?: string;
  size?: "sm" | "md" | "lg";
  style?: CSSProperties;
}

export function Combobox({ label, suggestions, value, onChange, placeholder, size = "md", style }: ComboboxProps) {
  const listId = useId();
  const optionId = (index: number) => `${listId}-option-${index}`;
  const [open, setOpen] = useState(false);
  // Which mode the popup was opened in — see `combobox-options.ts`. The
  // chevron and an arrow key open it `true`; typing drops it to `false`.
  const [browseAll, setBrowseAll] = useState(false);
  const [active, setActive] = useState(-1);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const activeRef = useRef<HTMLLIElement>(null);
  const [placement, setPlacement] = useState<PopupPlacement | null>(null);

  const options = visibleSuggestions(suggestions, value, browseAll);
  // A list that filtered down to nothing is not a popup, it is an empty box
  // over the field the reader is still typing into.
  const expanded = open && options.length > 0;

  function close() {
    setOpen(false);
    setActive(-1);
    // Dropped here rather than in the placing effect below, so that
    // effect only ever *measures*. A shut list is unmounted, so nothing
    // reads a stale placement in the meantime.
    setPlacement(null);
  }

  function openWith(all: boolean) {
    setBrowseAll(all);
    setOpen(true);
  }

  function commit(suggestion: string) {
    onChange(suggestion);
    close();
    inputRef.current?.focus();
  }

  // Published for the shell's Escape (`combobox-open.ts`'s header). Keyed on
  // `expanded`, so the withdrawal covers closing and unmounting-while-open
  // alike — an item panel torn down with its list up must not leave the
  // shell believing something is still open.
  useEffect(() => {
    if (!expanded) {
      return;
    }
    return comboboxOpenSignal.register(close);
  }, [expanded]);

  // Outside pointers close the list. `pointerdown` rather than `click`: on a
  // press that lands on another control the list should be gone before that
  // control acts, and a `click` fires too late for that to be true.
  useEffect(() => {
    if (!expanded) {
      return;
    }
    function onPointerDown(event: PointerEvent) {
      const target = event.target;
      if (target instanceof Node && wrapperRef.current?.contains(target)) {
        return;
      }
      close();
    }
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [expanded]);

  // Placed from a measurement, because the popup is `position: fixed` —
  // `combobox-placement.ts` carries the argument for that and the rule for
  // where it lands. Re-measured on scroll (capture phase, so the capture
  // popover's own scrolling dialog is caught, not just the window) and on
  // resize: a fixed popup does not follow the field it belongs to.
  //
  // `useLayoutEffect` so the measurement happens before the browser paints:
  // under `useEffect` the popup's first frame is a list at (0, 0), which is
  // why it renders `visibility: hidden` until placed.
  useLayoutEffect(() => {
    if (!expanded) {
      return;
    }
    function place() {
      const box = wrapperRef.current?.getBoundingClientRect();
      if (box) {
        setPlacement(popupPlacement(box, window.innerHeight, POPUP_METRICS));
      }
    }
    place();
    window.addEventListener("scroll", place, true);
    window.addEventListener("resize", place);
    return () => {
      window.removeEventListener("scroll", place, true);
      window.removeEventListener("resize", place);
    };
  }, [expanded]);

  // The active option follows the keyboard out of view otherwise: the popup
  // scrolls at seven-ish options and the vocabulary is already longer than
  // that. Optional-called because jsdom implements no scrolling at all.
  useEffect(() => {
    activeRef.current?.scrollIntoView?.({ block: "nearest" });
  }, [active]);

  function onKeyDown(event: ReactKeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!expanded) {
        // An arrow into a shut list browses, for the same reason the chevron
        // does: the sticky value must not decide what can be reached.
        openWith(true);
        setActive(0);
        return;
      }
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((current) => (current + step + options.length) % options.length);
      return;
    }
    if (event.key === "Enter" && expanded && active >= 0) {
      // The capture box reads Enter as "capture to Triage" from an enclosing
      // handler. While an option is active the key belongs to the list, and
      // only then — an Enter with nothing active is still the reader
      // submitting what they typed.
      event.preventDefault();
      event.stopPropagation();
      commit(options[active]);
      return;
    }
    if (event.key === "Tab" && expanded) {
      // Leaving the field commits nothing. Tab is navigation here, never a
      // silent accept of whatever happened to be highlighted.
      close();
    }
  }

  return (
    // The wrapper carries `onBlur` to notice focus leaving the control as a
    // whole (field ↔ chevron is not a dismissal). That is focus bookkeeping,
    // not an interaction: there is nothing here to operate, and the roles
    // and keys live on the `<Input>` and its listbox below.
    // eslint-disable-next-line jsx-a11y/no-static-element-interactions
    <div ref={wrapperRef} style={{ position: "relative", ...style }}
      onBlur={(event) => {
        // Focus leaving the control entirely — Tab, or a click on something
        // that takes focus. A move *within* it (field ↔ chevron) is not a
        // dismissal.
        const next = event.relatedTarget;
        if (next instanceof Node && wrapperRef.current?.contains(next)) {
          return;
        }
        close();
      }}>
      <Input
        label={label}
        size={size}
        value={value}
        placeholder={placeholder}
        inputRef={inputRef}
        role="combobox"
        aria-expanded={expanded}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={expanded && active >= 0 ? optionId(active) : undefined}
        autoComplete="off"
        onKeyDown={onKeyDown}
        onChange={(event) => {
          onChange(event.target.value);
          openWith(false);
          setActive(-1);
        }}
        trailing={
          <button type="button" tabIndex={-1} aria-label={label ? `Show ${label.toLowerCase()} suggestions` : "Show suggestions"}
            // `mousedown` is where the focus loss would happen; preventing it
            // there keeps the caret in the field, so the chevron reads as a
            // disclosure on the field rather than as a control of its own.
            // It is `tabIndex={-1}` for the same reason: the keyboard route
            // into the list is ArrowDown, and a second tab stop in the middle
            // of the capture form would be a tax on everyone else.
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => {
              // The chevron means "show me everything", and only shuts the
              // list when everything is already showing. Toggling on `open`
              // alone would make it a no-op mid-type — the one moment the
              // reader most wants out of their own filter, and the moment the
              // old `<datalist>` had no answer for.
              if (expanded && browseAll) {
                close();
              } else {
                openWith(true);
                setActive(-1);
              }
              inputRef.current?.focus();
            }}
            style={{ display: "flex", alignItems: "center", padding: 0, margin: 0, border: "none",
              background: "transparent", color: "var(--text-muted)", cursor: "pointer" }}>
            <Icon name="chevron-down" size={16} />
          </button>
        }
      />
      {/* No `aria-label`: the popup is named by the combobox that owns
          it (`aria-controls`), and a second element carrying the field's
          label makes "Context" ambiguous to every by-label query — the
          test-library ones included. */}
      {expanded ? (
        <ul id={listId} role="listbox"
          style={{ position: "fixed", top: placement?.top ?? 0, left: placement?.left ?? 0,
            width: placement?.width ?? 0, maxHeight: placement?.maxHeight ?? POPUP_METRICS.preferredHeight,
            // Over the capture popover's own scrim and card (both z-index
            // 40, `shell/CapturePopover.tsx`), since it opens inside one.
            zIndex: 50, visibility: placement ? "visible" : "hidden",
            margin: 0, padding: "var(--space-2)", listStyle: "none", overflowY: "auto",
            background: "var(--surface-card)", border: "1px solid var(--border-default)",
            borderRadius: "var(--radius-control)", boxShadow: "var(--shadow-3)" }}>
          {options.map((suggestion, index) => (
            // No key handler *on the option*, by the pattern: listbox options
            // are not focusable and never receive keys. The input keeps focus
            // and owns traversal and commit (`onKeyDown` above), pointing at
            // the active option through `aria-activedescendant`.
            // eslint-disable-next-line jsx-a11y/click-events-have-key-events
            <li key={suggestion} id={optionId(index)} role="option" aria-selected={index === active}
              ref={index === active ? activeRef : undefined}
              onMouseDown={(event) => event.preventDefault()}
              onMouseEnter={() => setActive(index)}
              onClick={() => commit(suggestion)}
              style={{ display: "flex", alignItems: "center", minHeight: 44, padding: `0 var(--space-4)`,
                borderRadius: "var(--radius-sm)", cursor: "pointer",
                font: "var(--type-body)", color: "var(--text-primary)",
                background: index === active ? "var(--surface-quiet)" : "transparent" }}>
              {suggestion}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
