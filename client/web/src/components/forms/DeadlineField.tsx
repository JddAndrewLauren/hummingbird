import { useState } from "react";
import { Button } from "../core/Button";
import { IconButton } from "../core/IconButton";
import { Input } from "./Input";
import { joinDeadline, splitDeadline } from "../../screens/deadline-parts";

// The one control that edits an item's deadline: a date picker, and a time
// picker that appears only when the deadline actually names a minute.
//
// **Why the time is behind a button.** Most deadlines are days — "the passport
// renewal is due on the 9th" — and a permanently visible time picker asks a
// question the reader usually has no answer to, then shows `00:00` when they
// decline to answer it. So the resting form is a date, and naming an hour is a
// deliberate second gesture. Clearing the time is the same gesture reversed
// (the × inside the time field), which puts the deadline back to a whole day
// rather than to midnight.
//
// The value is one string throughout, exactly as the wire carries it
// (`YYYY-MM-DD` or `YYYY-MM-DDTHH:MM`) — `deadline-parts.ts` does the whole of
// the splitting and joining, so this component holds no parsed state that could
// disagree with the value it was given. Its only state is whether the time
// picker is *revealed*, which is a fact about the form and not about the item:
// a revealed-but-empty time picker is a reader mid-decision.

export interface DeadlineFieldProps {
  value: string;
  onChange: (value: string) => void;
  label?: string;
  error?: string;
  size?: "sm" | "md" | "lg";
  disabled?: boolean;
}

export function DeadlineField({
  value,
  onChange,
  label = "Deadline",
  error,
  size = "sm",
  disabled,
}: DeadlineFieldProps) {
  const parts = splitDeadline(value);
  const [revealed, setRevealed] = useState(false);
  const showTime = parts.time !== null || revealed;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <Input
        label={label}
        size={size}
        type="date"
        value={parts.date}
        error={error}
        disabled={disabled}
        onChange={(event) => onChange(joinDeadline(event.target.value, parts.time))}
      />
      {showTime ? (
        <Input
          label="Time"
          size={size}
          type="time"
          value={parts.time ?? ""}
          disabled={disabled}
          onChange={(event) => onChange(joinDeadline(parts.date, event.target.value))}
          trailing={
            <IconButton
              icon="x"
              label="Remove the time"
              size="sm"
              variant="ghost"
              disabled={disabled}
              onClick={() => {
                setRevealed(false);
                onChange(joinDeadline(parts.date, null));
              }}
            />
          }
        />
      ) : (
        <div>
          <Button
            size="sm"
            variant="ghost"
            iconLeft="clock"
            disabled={disabled}
            onClick={() => setRevealed(true)}
          >
            Add time
          </Button>
        </div>
      )}
    </div>
  );
}
