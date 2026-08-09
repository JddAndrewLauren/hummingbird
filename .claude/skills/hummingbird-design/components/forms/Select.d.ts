import * as React from "react";
export interface SelectOption { value: string; label: string }
export interface SelectProps extends Omit<React.SelectHTMLAttributes<HTMLSelectElement>, "size" | "style"> {
  label?: string;
  /** Strings are used as both value and label. */
  options?: Array<string | SelectOption>;
  size?: "sm" | "md" | "lg";
  style?: React.CSSProperties;
}
export declare function Select(props: SelectProps): JSX.Element;
