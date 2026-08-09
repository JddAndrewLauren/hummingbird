import * as React from "react";
export interface CheckboxProps extends Omit<React.LabelHTMLAttributes<HTMLLabelElement>, "onChange" | "style"> {
  checked?: boolean;
  onChange?: React.ChangeEventHandler<HTMLInputElement>;
  label?: React.ReactNode;
  /** Second line under the label, muted. */
  hint?: React.ReactNode;
  /** "warn" turns the label amber — used for an unavailable-but-selected calendar. */
  tone?: "default" | "warn";
  disabled?: boolean;
  style?: React.CSSProperties;
}
export declare function Checkbox(props: CheckboxProps): JSX.Element;
