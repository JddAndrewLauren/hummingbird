import * as React from "react";
export interface ButtonProps extends Omit<React.ButtonHTMLAttributes<HTMLButtonElement>, "style"> {
  /** primary = the one committing action per view. quiet = brand-tinted secondary. */
  variant?: "primary" | "secondary" | "ghost" | "quiet" | "danger";
  /** lg is the touch size (44px); md is desktop default. */
  size?: "sm" | "md" | "lg";
  /** Lucide icon name placed before the label. */
  iconLeft?: string;
  /** Lucide icon name placed after the label. */
  iconRight?: string;
  loading?: boolean;
  disabled?: boolean;
  fullWidth?: boolean;
  style?: React.CSSProperties;
  children?: React.ReactNode;
}
export declare function Button(props: ButtonProps): JSX.Element;
