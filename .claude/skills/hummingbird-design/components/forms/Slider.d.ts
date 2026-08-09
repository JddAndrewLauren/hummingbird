import * as React from "react";
export interface SliderProps extends Omit<React.HTMLAttributes<HTMLDivElement>, "style" | "onChange"> {
  label: string;
  /** Discrete stop labels, left to right (e.g. ["low","medium","high"]). */
  options?: string[];
  /** Index into options, or null for the unset state. */
  value?: number | null;
  onChange?: (value: number | null) => void;
  /** Shows "not set" and a clear (×); unset is a legitimate resting state. Default true. */
  optional?: boolean;
  style?: React.CSSProperties;
}
export declare function Slider(props: SliderProps): JSX.Element;
