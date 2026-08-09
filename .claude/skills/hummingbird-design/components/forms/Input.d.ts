import * as React from "react";
export interface InputProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "size" | "style"> {
  label?: string;
  /** Helper line under the field; replaced by `error` when set. */
  hint?: string;
  error?: string;
  /** Lucide icon name rendered inside the field, leading. */
  icon?: string;
  size?: "sm" | "md" | "lg";
  /** Node pinned to the right inside the field (a key hint, a clear button). */
  trailing?: React.ReactNode;
  style?: React.CSSProperties;
}
export declare function Input(props: InputProps): JSX.Element;
