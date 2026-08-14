import { useId } from "react";
import type { CSSProperties } from "react";
import { Icon } from "../core/Icon";
import { Input } from "./Input";

// A text field with suggestions: anything is typeable, and a named few are one
// click away. The form `Select` cannot do this — a `<select>`'s values *are*
// its vocabulary — so a field whose vocabulary is open in the schema has to be
// an input, or the client silently closes it (see `screens/field-vocabulary.ts`
// for the decision this exists to serve).
//
// Native `<datalist>` rather than a listbox of our own. The popup is browser
// chrome and takes none of the design tokens, which is the one visible cost;
// what it buys is every behaviour a hand-rolled combobox has to reimplement
// and usually gets wrong — filter-as-you-type, keyboard traversal, the mobile
// picker, and the accessibility contract, which arrives for free because
// `input[list]` is already `role="combobox"` to a screen reader.
//
// The trailing `chevron-down` is borrowed from `Select` deliberately: without
// it the control is indistinguishable from a plain text field, and a reader who
// cannot see that suggestions exist will not go looking for them. It carries no
// `title`, so `Icon` leaves it `aria-hidden`, and it takes no pointer events —
// the glyph is a signal, not a button, since only the browser can open its own
// popup.

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
  return (
    <>
      <Input
        label={label}
        size={size}
        style={style}
        value={value}
        placeholder={placeholder}
        list={listId}
        onChange={(event) => onChange(event.target.value)}
        trailing={<Icon name="chevron-down" size={16} color="var(--text-muted)" style={{ pointerEvents: "none" }} />}
      />
      <datalist id={listId}>
        {suggestions.map((suggestion) => (
          <option key={suggestion} value={suggestion} />
        ))}
      </datalist>
    </>
  );
}
